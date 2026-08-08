//! DOM + ComputedStyle → LayoutTree 转换。
//!
//! [`build_layout_tree`] 递归遍历 DOM 树，为每个未被 `display:none` 排除的
//! Element 节点创建 taffy 节点，构建 [`LayoutTree`]。
//!
//! # 规范依据
//!
//! - CSS Display Level 3 §2: box tree generation
//! - CSS Box Model Level 3 §2: box generation

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use muskitty_cascade::ComputedStyle;
use muskitty_dom::Node;
use taffy::NodeId;

use crate::style_map;
use crate::tree::LayoutTree;

/// DOM 节点指针地址 → ComputedStyle 的映射类型。
pub type StyleMap = HashMap<usize, ComputedStyle>;

/// 从 DOM 树 + ComputedStyle 表构建 [`LayoutTree`]。
///
/// 递归遍历 DOM：
/// - [`Element`](muskitty_dom::NodeKind::Element) 节点 → 映射 ComputedStyle 为
///   taffy [`Style`](taffy::style::Style) 并创建 taffy 节点。
/// - `display: none` 的元素及其子树被跳过（不生成布局盒）。
/// - `display: contents` 的元素不生成盒，其子元素直接参与祖父格式上下文
///   （P2-12，CSS Display L3 §2.5）。
/// - head/title/script/style 等非渲染标签不生成盒（P2-13）。
/// - Text / Comment / Document 等非元素节点跳过（文本测量推迟到后续批次）。
///
/// DOM 节点的 `Rc` 指针地址（`Rc::as_ptr(node) as usize`）作为 [`LayoutTree::node_map`]
/// 的 key，供布局结果查询时关联回 DOM 节点。
pub fn build_layout_tree(root: &Rc<RefCell<Node>>, styles: &StyleMap) -> LayoutTree {
    let mut tree = LayoutTree::new();
    // 如果根节点是 Element，直接构建；否则查找第一个 Element 子节点
    // （HTML 解析的根是 Document 节点，需要找到 <html> 元素）
    let root_element = find_root_element(root);
    if let Some(root_el) = root_element {
        // 根元素可能 display:none / contents 而不生成根盒；取首个生成的 box
        // 作为树根（contents 根极罕见，防御处理）。
        let roots = build_node_recursive(&mut tree, &root_el, styles);
        tree.root = roots.first().copied();
    }
    tree
}

/// 查找 DOM 树的根元素。
///
/// 如果 `node` 本身是 Element，直接返回；否则递归查找第一个 Element 子节点。
/// 用于跳过 Document/Doctype 等非元素根节点。
fn find_root_element(node: &Rc<RefCell<Node>>) -> Option<Rc<RefCell<Node>>> {
    if matches!(node.borrow().kind, muskitty_dom::NodeKind::Element(_)) {
        return Some(Rc::clone(node));
    }
    for child in node.borrow().child_nodes() {
        if let Some(found) = find_root_element(child) {
            return Some(found);
        }
    }
    None
}

/// 递归为 DOM 子树构建 taffy 节点。
///
/// 返回该 DOM 子树生成的所有 taffy box：
/// - 非元素 / `display: none` / 非渲染标签 → 空（不生成盒）。
/// - `display: contents` → 元素不生成盒，其子元素直接参与祖父格式上下文，
///   返回子元素生成的 box（P2-12）。
/// - 普通元素 → 生成一个盒，返回 `[id]`。
fn build_node_recursive(
    tree: &mut LayoutTree,
    node: &Rc<RefCell<Node>>,
    styles: &StyleMap,
) -> Vec<NodeId> {
    // 先收集节点信息并释放 node 的借用，避免在递归期间持有 RefCell 借用。
    let (addr, computed, children, is_contents) = {
        let node_ref = node.borrow();
        let kind = &node_ref.kind;
        let (addr, tag) = match kind {
            muskitty_dom::NodeKind::Element(el) => (
                Rc::as_ptr(node) as usize,
                el.local_name.to_ascii_lowercase(),
            ),
            _ => return Vec::new(),
        };

        let computed = styles.get(&addr);

        // P2-13: head/title/script/style/meta/link/base/template 等非渲染标签
        // 不生成布局盒（无 UA 表时默认，避免多余空白盒）。
        if is_non_rendered_tag(&tag) {
            return Vec::new();
        }

        // display 关键字（单态化后取首个 Ident，P2-20）。
        let display_kw = computed
            .and_then(|cs| cs.get("display"))
            .and_then(|cv| cv.keyword())
            .map(|s| s.to_ascii_lowercase());

        // display: none → 跳过该节点及其整个子树。
        if display_kw.as_deref() == Some("none") {
            return Vec::new();
        }

        let is_contents = display_kw.as_deref() == Some("contents");

        // 克隆子节点 Rc 引用，释放 node_ref 借用后再递归。
        let children: Vec<Rc<RefCell<Node>>> = node_ref.child_nodes().to_vec();
        (addr, computed, children, is_contents)
    };

    // display: contents → 元素不生成盒，子元素直接参与父格式上下文，
    // 返回子元素生成的 box 列表，由父节点拼接（P2-12）。
    if is_contents {
        return children
            .iter()
            .flat_map(|child| build_node_recursive(tree, child, styles))
            .collect();
    }

    let taffy_style = style_map::map_style(computed);

    // 先递归处理子节点，收集已创建的 taffy NodeId 列表。
    let child_ids: Vec<NodeId> = children
        .iter()
        .flat_map(|child| build_node_recursive(tree, child, styles))
        .collect();

    // 根据是否有子节点选择创建方式。
    let taffy_node = if child_ids.is_empty() {
        tree.taffy
            .new_leaf(taffy_style)
            .expect("taffy new_leaf 失败：无法创建叶子布局节点")
    } else {
        tree.taffy
            .new_with_children(taffy_style, &child_ids)
            .expect("taffy new_with_children 失败：无法创建带子节点的布局节点")
    };

    tree.node_map.insert(addr, taffy_node);
    vec![taffy_node]
}

/// 非渲染元素标签：这些标签不产生可视化盒（head 及其元数据、脚本、样式等）。
///
/// 无 UA 样式表时给它们建布局盒会生成多余的空白盒（P2-13）。
fn is_non_rendered_tag(tag: &str) -> bool {
    matches!(
        tag,
        "head" | "title" | "script" | "style" | "meta" | "link" | "base" | "template"
    )
}
