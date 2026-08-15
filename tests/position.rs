//! L-1 position 定位集成测试。
//!
//! 覆盖 `position: relative` / `absolute`（`fixed` 近似为 absolute）的
//! containing block 语义（CSS Positioned Layout Level 3 §2-§4）。

use muskitty_cascade::{compute_styles, StyleTreeOptions};
use muskitty_css::parse_stylesheet;
use muskitty_cssom::{from_stylesheet, Origin};
use muskitty_dom::{Node, NodeKind};
use muskitty_layout::{build_layout_tree, compute_layout};
use std::cell::RefCell;
use std::rc::Rc;

fn full_pipeline(html: &str, css: &str) -> (Rc<RefCell<Node>>, muskitty_layout::LayoutResult) {
    let dom = muskitty_html5_parser::parse(html);
    let parsed = parse_stylesheet(css);
    let sheet = {
        let mut s = from_stylesheet(&parsed);
        s.origin = Origin::Author;
        s
    };
    let styles = compute_styles(&dom, &[sheet], &StyleTreeOptions::default());
    let mut tree = build_layout_tree(&dom, &styles);
    let layout = compute_layout(&mut tree, 800.0, 600.0).expect("layout ok");
    (dom, layout)
}

fn find_addr_by_id(node: &Rc<RefCell<Node>>, id: &str) -> Option<usize> {
    if let NodeKind::Element(el) = &node.borrow().kind {
        if el
            .attributes
            .iter()
            .any(|a| a.local_name == "id" && a.value == id)
        {
            return Some(Rc::as_ptr(node) as usize);
        }
    }
    for child in node.borrow().child_nodes() {
        if let Some(a) = find_addr_by_id(child, id) {
            return Some(a);
        }
    }
    None
}

fn assert_abs(
    dom: &Rc<RefCell<Node>>,
    layout: &muskitty_layout::LayoutResult,
    id: &str,
    x: f32,
    y: f32,
) {
    let addr = find_addr_by_id(dom, id).unwrap_or_else(|| panic!("element #{id} not found"));
    let l = layout
        .get(addr)
        .unwrap_or_else(|| panic!("layout missing for #{id}"));
    assert!(
        (l.abs_x - x).abs() < 0.5,
        "#{id} abs_x should be {x}, got {}",
        l.abs_x
    );
    assert!(
        (l.abs_y - y).abs() < 0.5,
        "#{id} abs_y should be {y}, got {}",
        l.abs_y
    );
}

#[test]
fn absolute_direct_child_of_relative() {
    // parent(relative) 是 positioned ancestor，abs 直接子元素相对其定位。
    let (dom, layout) = full_pipeline(
        "<div id=\"p\" style=\"position: relative; width: 300px; height: 200px\"><div id=\"abs\" style=\"position: absolute; top: 10px; left: 20px; width: 50px; height: 30px\"></div></div>",
        "div { display: block; } body { margin: 0; }",
    );
    assert_abs(&dom, &layout, "abs", 20.0, 10.0);
}

#[test]
fn absolute_relative_to_positioned_ancestor_not_dom_parent() {
    // container(relative) 是 containing block，mid(static) 不是。abs 相对
    // container 定位，不受 mid 的 margin 影响（L-1：absolute 重挂载）。
    let (dom, layout) = full_pipeline(
        "<div id=\"container\" style=\"position: relative; width: 300px; height: 200px; padding-top: 20px\"><div id=\"mid\" style=\"width: 200px; height: 100px; margin-top: 50px\"><div id=\"abs\" style=\"position: absolute; top: 10px; left: 20px; width: 50px; height: 30px\"></div></div></div>",
        "div { display: block; } body { margin: 0; }",
    );
    assert_abs(&dom, &layout, "abs", 20.0, 10.0);
}

#[test]
fn relative_offset_applies() {
    // relative 元素用 top/left 偏移自身（不移动其他元素）。
    let (dom, layout) = full_pipeline(
        "<div><div id=\"rel\" style=\"position: relative; top: 10px; left: 20px; width: 50px; height: 30px\"></div></div>",
        "div { display: block; } body { margin: 0; }",
    );
    assert_abs(&dom, &layout, "rel", 20.0, 10.0);
}

#[test]
fn static_default_no_offset() {
    // 默认 static，无 inset 偏移。
    let (dom, layout) = full_pipeline(
        "<div><div id=\"s\" style=\"width: 50px; height: 30px\"></div></div>",
        "div { display: block; } body { margin: 0; }",
    );
    assert_abs(&dom, &layout, "s", 0.0, 0.0);
}
