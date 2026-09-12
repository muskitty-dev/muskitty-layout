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
use muskitty_dom::{Node, NodeKind};
use taffy::NodeId;

use crate::style_map;
use crate::text::{
    resolve_font_family, resolve_font_size, resolve_font_weight, DEFAULT_FONT_FAMILY,
    DEFAULT_FONT_SIZE, DEFAULT_FONT_WEIGHT,
};
use crate::tree::{LayoutTree, NodeContext, SharedFontSystem};

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
    build_layout_tree_with_fonts(root, styles, &SharedFontSystem::new())
}

/// 同 [`build_layout_tree`]，但注入会话级共享字体系统（LAY-2）。
///
/// `FontSystem::new()` 枚举系统字体需 50–300 ms；页面/会话级持有一份
/// [`SharedFontSystem`]，每次建树/resize/热重载注入复用，不再重复支付。
pub fn build_layout_tree_with_fonts(
    root: &Rc<RefCell<Node>>,
    styles: &StyleMap,
    fonts: &SharedFontSystem,
) -> LayoutTree {
    let mut tree = LayoutTree::new_with_fonts(fonts);
    // 如果根节点是 Element，直接构建；否则查找第一个 Element 子节点
    // （HTML 解析的根是 Document 节点，需要找到 <html> 元素）
    let root_element = find_root_element(root);
    if let Some(root_el) = root_element {
        // 根元素可能 display:none / contents 而不生成根盒；取首个生成的 box
        // 作为树根（contents 根极罕见，防御处理）。FontSystem 由 LayoutTree
        // 持有（T-3 换行 measure function 在 compute_layout 时使用）。
        let built = build_node_recursive(
            &mut tree,
            &root_el,
            styles,
            &InheritedText {
                font_size: DEFAULT_FONT_SIZE,
                font_family: DEFAULT_FONT_FAMILY,
                font_weight: DEFAULT_FONT_WEIGHT,
                line_height: DEFAULT_FONT_SIZE * muskitty_cascade::NORMAL_LINE_HEIGHT,
                text_transform: None,
            },
        );
        tree.root = built.in_flow.first().copied();
        // 子树内无 positioned ancestor 的 absolute box 挂到根盒（html），
        // taffy 相对根定位 = 相对 viewport origin（L-1）。
        if let Some(root_id) = tree.root {
            for abs in &built.absolute {
                tree.taffy.add_child(root_id, *abs).ok();
            }
        }
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

/// 构建结果：区分正常流 box 与待挂载的 absolute box。
///
/// absolute 元素从 normal flow 移除，其 box 通过返回值上传到最近
/// positioned ancestor 挂载（CSS Positioned Layout Level 3 §3: containing block）。
#[derive(Default)]
struct Built {
    /// 正常流 box（作为 DOM 父节点的 taffy 子节点）。
    in_flow: Vec<NodeId>,
    /// absolute box（挂到最近 positioned ancestor）。
    absolute: Vec<NodeId>,
}

/// 递归下传的**继承文本上下文**（T-3 字体属性 / M-3 batch 3 行高与转换）。
///
/// 这些值对元素子树生效（继承属性），text 叶节点直接取其快照做测量。
/// 收成结构体而非逐个参数：T-3 与 batch 3 已两次扩列，后续
/// `white-space` / `direction` 还要再加（clippy 8 参数上限）。
struct InheritedText<'a> {
    /// 继承的 font-size（px 使用值）。
    font_size: f32,
    /// 继承的 font-family（首个族名）。
    font_family: &'a str,
    /// 继承的 font-weight（100-900）。
    font_weight: u16,
    /// 继承的 line-height（px 使用值，cascade `text_props` 解析）。
    line_height: f32,
    /// 继承的 text-transform 关键字（`None` = 未声明，等同 `none`）。
    text_transform: Option<&'a str>,
}

/// 递归为 DOM 子树构建 taffy 节点。
///
/// 返回 [`Built`]：
/// - Text 节点 → 测量为固定尺寸 leaf（T-1）。
/// - Comment / Document 等非元素 / `display: none` / 非渲染标签 → 空。
/// - `display: contents` → 元素不生成盒，子 in_flow box 上浮（P2-12）。
/// - 普通元素 → 生成一个盒；`position: absolute/fixed` 的盒进入 `absolute`
///   列表，由最近 positioned ancestor 挂载（L-1）。
fn build_node_recursive(
    tree: &mut LayoutTree,
    node: &Rc<RefCell<Node>>,
    styles: &StyleMap,
    inherited: &InheritedText<'_>,
) -> Built {
    // —— Text 节点：携带 context 的 leaf，布局时 measure function 换行测量（T-3）——
    // Text 是叶子，无子递归；不预先测量，尺寸由 compute_layout 的 measure
    // function 按容器可用宽度确定。
    {
        let node_ref = node.borrow();
        if let NodeKind::Text(text) = &node_ref.kind {
            // 纯空白文本节点（源码换行/缩进）不参与布局：块级边界空白按
            // CSS white-space 处理应为零尺寸。否则每处源码换行都会生成
            // 实体文本盒（且换行符被 cosmic-text 视为换行 → 2 行高），把
            // 后续内容逐级下推（"汉字位移"的第二个来源）。行内空格的
            // 折叠属于 inline formatting context（远期）。
            if text.data.chars().all(|c| c.is_ascii_whitespace()) {
                return Built {
                    in_flow: vec![],
                    absolute: vec![],
                };
            }
            let addr = Rc::as_ptr(node) as usize;
            // M-3 batch 3：`text-transform` 在**布局前**生效（CSS Text L3 §2.1
            // 的转换改变用于排版与绘制的文本）。此处存转换后文本，保证测量
            // 与 renderer 绘制看到同一份内容（缓存键亦随之自洽）。
            let transformed =
                muskitty_cascade::apply_text_transform(&text.data, inherited.text_transform);
            let leaf = tree
                .taffy
                .new_leaf_with_context(
                    taffy::style::Style::default(),
                    NodeContext::Text {
                        text: transformed.into_owned(),
                        font_size: inherited.font_size,
                        font_family: inherited.font_family.to_string(),
                        font_weight: inherited.font_weight,
                        line_height: inherited.line_height,
                        // LAY-3：测量缓存冷启动（首次 measure 填充）。
                        measured: None,
                    },
                )
                .expect("taffy new_leaf_with_context 失败：无法创建文本叶子节点");
            tree.node_map.insert(addr, leaf);
            return Built {
                in_flow: vec![leaf],
                absolute: vec![],
            };
        }
    }

    // —— Element 节点：先收集信息并释放 node 的借用，避免在递归期间持有 ——
    let (
        addr,
        computed,
        children,
        is_contents,
        own_font_size,
        own_font_family,
        own_font_weight,
        own_line_height,
        own_text_transform,
        is_absolute,
        is_positioned,
    ) = {
        let node_ref = node.borrow();
        let (addr, tag) = match &node_ref.kind {
            NodeKind::Element(el) => (
                Rc::as_ptr(node) as usize,
                el.local_name.to_ascii_lowercase(),
            ),
            _ => return Built::default(),
        };

        let computed = styles.get(&addr);

        // P2-13: head/title/script/style/meta/link/base/template 等非渲染标签
        // 不生成布局盒（无 UA 表时默认，避免多余空白盒）。
        if is_non_rendered_tag(&tag) {
            return Built::default();
        }

        // display 关键字（单态化后取首个 Ident，P2-20）。
        let display_kw = computed
            .and_then(|cs| cs.get("display"))
            .and_then(|cv| cv.keyword())
            .map(|s| s.to_ascii_lowercase());

        // display: none → 跳过该节点及其整个子树。
        if display_kw.as_deref() == Some("none") {
            return Built::default();
        }

        let is_contents = display_kw.as_deref() == Some("contents");

        // 自身 font-size（px）：继承属性，子 text 节点用其测量。无显式声明
        // 时回退到继承值。
        let own_font_size = computed
            .and_then(resolve_font_size)
            .unwrap_or(inherited.font_size);

        // 自身 font-family / font-weight（继承属性，T-3）。
        let own_font_family = computed
            .and_then(resolve_font_family)
            .unwrap_or_else(|| inherited.font_family.to_string());
        let own_font_weight = computed
            .and_then(resolve_font_weight)
            .unwrap_or(inherited.font_weight);

        // 自身 line-height 使用值（继承属性，M-3 batch 3）：语义由 cascade
        // 单一来源给出（`normal`=1.2×font-size、数为倍数、百分比已在计算值
        // 阶段转 px）。无 computed style 的节点沿用继承值。
        let own_line_height = computed
            .map(|cs| muskitty_cascade::used_line_height_px(cs, own_font_size))
            .unwrap_or(inherited.line_height);

        // 自身 text-transform 关键字（继承属性，M-3 batch 3）。compute_styles
        // 已应用继承，故元素帧上必定可取到关键字（初始值 none）。
        let own_text_transform: Option<String> = computed
            .and_then(muskitty_cascade::text_transform_keyword)
            .map(str::to_string)
            .or_else(|| inherited.text_transform.map(str::to_string));

        // position 关键字（默认 static）。absolute/fixed → 脱离 normal flow；
        // absolute/fixed/relative 均为 positioned（成为子 absolute 的 containing block）。
        let position_kw = computed
            .and_then(|cs| cs.get("position"))
            .and_then(|cv| cv.keyword())
            .map(|s| s.to_ascii_lowercase());
        let is_absolute = matches!(position_kw.as_deref(), Some("absolute") | Some("fixed"));
        let is_positioned = is_absolute || matches!(position_kw.as_deref(), Some("relative"));

        // 克隆子节点 Rc 引用，释放 node_ref 借用后再递归。
        let children: Vec<Rc<RefCell<Node>>> = node_ref.child_nodes().to_vec();
        (
            addr,
            computed,
            children,
            is_contents,
            own_font_size,
            own_font_family,
            own_font_weight,
            own_line_height,
            own_text_transform,
            is_absolute,
            is_positioned,
        )
    };

    // 递归子节点，分别收集正常流与 absolute box。
    let mut in_flow_children: Vec<NodeId> = Vec::new();
    let mut absolute_desc: Vec<NodeId> = Vec::new();
    let child_inherited = InheritedText {
        font_size: own_font_size,
        font_family: &own_font_family,
        font_weight: own_font_weight,
        line_height: own_line_height,
        text_transform: own_text_transform.as_deref(),
    };
    for child in &children {
        let built = build_node_recursive(tree, child, styles, &child_inherited);
        in_flow_children.extend(built.in_flow);
        absolute_desc.extend(built.absolute);
    }

    // display: contents → 元素不生成盒，子 in_flow box 上浮（P2-12）。
    if is_contents {
        return Built {
            in_flow: in_flow_children,
            absolute: absolute_desc,
        };
    }

    let taffy_style = style_map::map_style(computed);
    let taffy_node = if in_flow_children.is_empty() {
        tree.taffy
            .new_leaf(taffy_style)
            .expect("taffy new_leaf 失败：无法创建叶子布局节点")
    } else {
        tree.taffy
            .new_with_children(taffy_style, &in_flow_children)
            .expect("taffy new_with_children 失败：无法创建带子节点的布局节点")
    };
    tree.node_map.insert(addr, taffy_node);

    if is_absolute {
        // 本元素 absolute：其后代 absolute 挂到本元素（本元素是 positioned），
        // 本元素自身作为 absolute box 上传到上层 positioned ancestor。
        for a in &absolute_desc {
            tree.taffy.add_child(taffy_node, *a).ok();
        }
        Built {
            in_flow: vec![],
            absolute: vec![taffy_node],
        }
    } else if is_positioned {
        // relative（positioned）：子树 absolute 后代挂到本元素（containing block）。
        for a in &absolute_desc {
            tree.taffy.add_child(taffy_node, *a).ok();
        }
        Built {
            in_flow: vec![taffy_node],
            absolute: vec![],
        }
    } else {
        // static：absolute 后代继续上传。
        Built {
            in_flow: vec![taffy_node],
            absolute: absolute_desc,
        }
    }
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
