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

use cosmic_text::FontSystem;
use muskitty_cascade::ComputedStyle;
use muskitty_dom::{Node, NodeKind};
use taffy::geometry::Size;
use taffy::style::{Dimension, Style};
use taffy::NodeId;

use crate::style_map;
use crate::text::{measure_text, resolve_font_size, DEFAULT_FONT_SIZE};
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
/// - Text 节点 → 测量为固定尺寸的 taffy leaf（T-1，单行不换行）。
/// - Comment / Document 等非元素节点跳过（无盒）。
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
        // 每棵布局树构建时创建一次 FontSystem（扫描系统字体），复用给所有
        // text 测量（T-1）。
        let mut font_system = FontSystem::new();
        let roots = build_node_recursive(
            &mut tree,
            &root_el,
            styles,
            &mut font_system,
            DEFAULT_FONT_SIZE,
        );
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
/// - Text 节点 → 测量为固定尺寸 leaf，返回 `[id]`（T-1）。
/// - Comment / Document 等非元素 / `display: none` / 非渲染标签 → 空（不生成盒）。
/// - `display: contents` → 元素不生成盒，其子元素直接参与祖父格式上下文，
///   返回子元素生成的 box（P2-12）。
/// - 普通元素 → 生成一个盒，返回 `[id]`。
fn build_node_recursive(
    tree: &mut LayoutTree,
    node: &Rc<RefCell<Node>>,
    styles: &StyleMap,
    font_system: &mut FontSystem,
    inherited_font_size: f32,
) -> Vec<NodeId> {
    // —— Text 节点：测量为固定尺寸的 taffy leaf（T-1，单行不换行）——
    // Text 是叶子，无子递归，可在持有 borrow 时直接测量（font_system 是
    // 独立参数，与 node 借用不冲突）。
    {
        let node_ref = node.borrow();
        if let NodeKind::Text(text) = &node_ref.kind {
            let (w, h) = measure_text(&text.data, inherited_font_size, font_system);
            let addr = Rc::as_ptr(node) as usize;
            let leaf = tree
                .taffy
                .new_leaf(Style {
                    size: Size {
                        width: Dimension::length(w),
                        height: Dimension::length(h),
                    },
                    ..Default::default()
                })
                .expect("taffy new_leaf 失败：无法创建文本叶子节点");
            tree.node_map.insert(addr, leaf);
            return vec![leaf];
        }
    }

    // —— Element 节点：先收集信息并释放 node 的借用，避免在递归期间持有 ——
    let (addr, computed, children, is_contents, own_font_size) = {
        let node_ref = node.borrow();
        let (addr, tag) = match &node_ref.kind {
            NodeKind::Element(el) => (
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

        // 自身 font-size（px）：继承属性，子 text 节点用其测量。无显式声明
        // 时回退到继承值。
        let own_font_size = computed
            .and_then(resolve_font_size)
            .unwrap_or(inherited_font_size);

        // 克隆子节点 Rc 引用，释放 node_ref 借用后再递归。
        let children: Vec<Rc<RefCell<Node>>> = node_ref.child_nodes().to_vec();
        (addr, computed, children, is_contents, own_font_size)
    };

    // display: contents → 元素不生成盒，子元素直接参与父格式上下文，
    // 返回子元素生成的 box 列表，由父节点拼接（P2-12）。
    if is_contents {
        return children
            .iter()
            .flat_map(|child| build_node_recursive(tree, child, styles, font_system, own_font_size))
            .collect();
    }

    let taffy_style = style_map::map_style(computed);

    // 先递归处理子节点，收集已创建的 taffy NodeId 列表。
    let child_ids: Vec<NodeId> = children
        .iter()
        .flat_map(|child| build_node_recursive(tree, child, styles, font_system, own_font_size))
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
