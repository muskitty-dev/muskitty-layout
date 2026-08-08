//! L-6 端到端集成测试：HTML + CSS → cascade → computed style → layout → result。
//!
//! 验证完整数据流：
//! ```text
//! HTML → DOM 树
//! CSS → CssStyleSheet[]
//!     → collect_declared_values (§5 Filtering)
//!     → cascade_for_element (§6.1 Cascade 排序)
//!     → cascade_winner (取首项)
//!     → apply_defaulting (§7 Defaulting)
//!     → compute_value (§4.4 Computed Value)
//!     → ComputedStyle per element
//!     → build_layout_tree
//!     → compute_layout
//!     → LayoutResult (per-element x/y/width/height)
//! ```

use muskitty_cascade::{compute_styles, StyleTreeOptions};
use muskitty_css::parse_stylesheet;
use muskitty_cssom::{from_stylesheet, Origin};
use muskitty_dom::{Node, NodeKind};
use muskitty_layout::{build_layout_tree, compute_layout};
use std::cell::RefCell;
use std::rc::Rc;

// —— 辅助函数 ——

/// 完整 pipeline 结果：包含 DOM 树和布局结果，供测试查询。
struct PipelineResult {
    dom: Rc<RefCell<Node>>,
    layout: muskitty_layout::LayoutResult,
}

/// 完整 pipeline：HTML + CSS → LayoutResult
fn full_pipeline(html: &str, css: &str, viewport_w: f32, viewport_h: f32) -> PipelineResult {
    // 1. 解析 HTML → DOM 树
    let dom = muskitty_html5_parser::parse(html);

    // 2. 解析 CSS → CssStyleSheet
    let parsed = parse_stylesheet(css);
    let sheet = {
        let mut s = from_stylesheet(&parsed);
        s.origin = Origin::Author;
        s
    };

    // 3. 遍历 DOM，对每个元素计算 ComputedStyle
    let styles = compute_styles(&dom, &[sheet], &StyleTreeOptions::default());

    // 4. 构建布局树
    let mut tree = build_layout_tree(&dom, &styles);

    // 5. 计算布局
    let layout = compute_layout(&mut tree, viewport_w, viewport_h).expect("layout should succeed");

    PipelineResult { dom, layout }
}

// —— 测试用例 ——

#[test]
fn simple_block_layout() {
    let result = full_pipeline(
        "<div><p>Hello</p></div>",
        "div { width: 300px; }",
        800.0,
        600.0,
    );
    assert!(
        !result.layout.nodes.is_empty(),
        "layout result should have nodes"
    );
}

#[test]
fn fixed_width_element() {
    let result = full_pipeline("<div style='width: 200px'></div>", "", 800.0, 600.0);
    let div_addr = find_element_by_tag(&result.dom, "div").expect("should find div element");
    let layout = result.layout.get(div_addr).expect("div should have layout");

    assert!(
        (layout.width - 200.0).abs() < 1.0,
        "width should be ~200px, got {}",
        layout.width
    );
}

#[test]
fn flex_layout_children_positioned() {
    let html = "<div style='display: flex'><div style='width: 100px'></div><div style='width: 200px'></div></div>";
    let result = full_pipeline(html, "", 800.0, 600.0);

    // 查找所有 div 元素，按 DOM 顺序排列
    let divs = collect_all_elements(&result.dom)
        .into_iter()
        .filter(|(_, name)| name.eq_ignore_ascii_case("div"))
        .map(|(addr, _)| addr)
        .collect::<Vec<_>>();

    // HTML 解析器会创建 html > body > div(flex) > div(100px) + div(200px)
    // 过滤掉 html/head/body 后应有 3 个 div
    assert!(
        divs.len() >= 3,
        "should find at least 3 div elements, got {}",
        divs.len()
    );

    let _parent = divs[0]; // 第一个 div 是 flex 容器
    let c1_addr = divs[1];
    let c2_addr = divs[2];

    let c1 = result
        .layout
        .get(c1_addr)
        .expect("child1 should have layout");
    let c2 = result
        .layout
        .get(c2_addr)
        .expect("child2 should have layout");

    assert!(
        (c1.width - 100.0).abs() < 1.0,
        "child1 width should be ~100px, got {}",
        c1.width
    );
    assert!(
        (c2.width - 200.0).abs() < 1.0,
        "child2 width should be ~200px, got {}",
        c2.width
    );
    // CSS Flexbox §4.5: flex row 中相邻 flex item 紧邻排列，
    // c2.x 应等于 c1.x + c1.width（无 gap/margin 时）。
    assert!(
        (c2.x - (c1.x + c1.width)).abs() < 1.0,
        "child2 x ({}) should be ~child1.x ({}) + child1.width ({}) = {}, got {}",
        c2.x,
        c1.x,
        c1.width,
        c1.x + c1.width,
        c2.x
    );
}

#[test]
fn percentage_width_resolved() {
    let result = full_pipeline("<div style='width: 50%'></div>", "", 800.0, 600.0);
    let div_addr = find_element_by_tag(&result.dom, "div").expect("should find div element");
    let layout = result.layout.get(div_addr).expect("div should have layout");

    assert!(
        (layout.width - 400.0).abs() < 1.0,
        "50% of 800px should be ~400px, got {}",
        layout.width
    );
}

#[test]
fn margin_applied_to_position() {
    let result = full_pipeline(
        "<div style='margin-left: 20px; width: 100px'></div>",
        "",
        800.0,
        600.0,
    );
    let div_addr = find_element_by_tag(&result.dom, "div").expect("should find div element");
    let layout = result.layout.get(div_addr).expect("div should have layout");

    assert!(
        (layout.width - 100.0).abs() < 1.0,
        "width should be ~100px, got {}",
        layout.width
    );
    // CSS Box Model §2.1: margin-left: 20px 应使元素 x 偏移 20px.
    // 使用精确断言（±1.0 容差）而非宽松的 x >= 19.0.
    assert!(
        (layout.x - 20.0).abs() < 1.0,
        "x should be ~20px (margin-left), got {}",
        layout.x
    );
}

#[test]
fn nested_block_layout() {
    let result = full_pipeline("<div><div><div></div></div></div>", "", 800.0, 600.0);
    // HTML 解析器创建 html > body > div > div > div，应有 3 个 div
    let div_count = collect_all_elements(&result.dom)
        .iter()
        .filter(|(_, name)| name.eq_ignore_ascii_case("div"))
        .count();
    assert!(
        div_count >= 3,
        "should have at least 3 div elements, got {}",
        div_count
    );
    assert!(
        result.layout.nodes.len() >= 3,
        "should have at least 3 layout nodes"
    );
}

#[test]
fn display_none_excluded() {
    let html = "<div><p style='display: none'></p></div>";
    let result = full_pipeline(html, "", 800.0, 600.0);

    let p_addr = find_element_by_tag(&result.dom, "p");
    if let Some(addr) = p_addr {
        assert!(
            result.layout.get(addr).is_none(),
            "display:none element should not be in layout result"
        );
    }
}

// —— DOM 辅助函数 —--

/// 递归收集 DOM 树中所有 Element 节点的地址和标签名。
fn collect_all_elements(node: &Rc<RefCell<Node>>) -> Vec<(usize, String)> {
    let mut result = vec![];
    {
        let borrowed = node.borrow();
        if let NodeKind::Element(elem) = &borrowed.kind {
            result.push((Rc::as_ptr(node) as usize, elem.local_name.clone()));
        }
    }
    for child in node.borrow().child_nodes() {
        result.extend(collect_all_elements(child));
    }
    result
}

/// 查找 DOM 树中第一个指定标签名的 Element 地址。
fn find_element_by_tag(node: &Rc<RefCell<Node>>, tag: &str) -> Option<usize> {
    for (addr, name) in collect_all_elements(node) {
        if name.eq_ignore_ascii_case(tag) {
            return Some(addr);
        }
    }
    None
}

/// 查找第一个有 Element 子节点的指定标签名元素及其子地址列表。
#[allow(dead_code)]
fn find_flex_parent_with_children(node: &Rc<RefCell<Node>>) -> (Option<usize>, Vec<usize>) {
    let all = collect_all_elements(node);
    for (addr, _) in &all {
        // 检查是否有 Element 子节点
        for child in node.borrow().child_nodes() {
            if matches!(child.borrow().kind, NodeKind::Element(_)) {
                // 找到有子元素的节点，收集其 Element 子节点地址
                let child_addrs: Vec<usize> = node
                    .borrow()
                    .child_nodes()
                    .iter()
                    .filter(|c| matches!(c.borrow().kind, NodeKind::Element(_)))
                    .map(|c| Rc::as_ptr(c) as usize)
                    .collect();
                if !child_addrs.is_empty() {
                    return (Some(*addr), child_addrs);
                }
            }
        }
    }
    // 递归搜索
    for child in node.borrow().child_nodes() {
        let result = find_flex_parent_with_children(child);
        if result.0.is_some() {
            return result;
        }
    }
    (None, vec![])
}
