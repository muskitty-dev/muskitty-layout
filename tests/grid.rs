//! L-3 grid 布局集成测试。
//!
//! 覆盖 `display: grid` + `grid-template-columns/rows` 的列/行布局
//! （CSS Grid Layout Level 1 §7）。

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

fn layout_of(
    dom: &Rc<RefCell<Node>>,
    layout: &muskitty_layout::LayoutResult,
    id: &str,
) -> muskitty_layout::NodeLayout {
    let addr = find_addr_by_id(dom, id).unwrap_or_else(|| panic!("element #{id} not found"));
    *layout
        .get(addr)
        .unwrap_or_else(|| panic!("layout missing for #{id}"))
}

#[test]
fn grid_two_columns_fixed_px() {
    // grid-template-columns: 100px 200px → 两列，item b 从 x=100 开始。
    let (dom, layout) = full_pipeline(
        "<div id=\"grid\" style=\"display: grid; grid-template-columns: 100px 200px\"><div id=\"a\" style=\"height: 20px\"></div><div id=\"b\" style=\"height: 20px\"></div></div>",
        "body { margin: 0; }",
    );
    let a = layout_of(&dom, &layout, "a");
    let b = layout_of(&dom, &layout, "b");
    assert!(
        (a.abs_x - 0.0).abs() < 0.5,
        "item a should start at x=0, got {}",
        a.abs_x
    );
    assert!(
        (b.abs_x - 100.0).abs() < 0.5,
        "item b should start at x=100 (second column), got {}",
        b.abs_x
    );
    assert!(
        (b.width - 200.0).abs() < 0.5,
        "item b column width 200px, got {}",
        b.width
    );
}

#[test]
fn grid_two_rows_fixed_px() {
    // grid-template-rows: 30px 40px → 两行，item b 从 y=30 开始。
    let (dom, layout) = full_pipeline(
        "<div id=\"grid\" style=\"display: grid; grid-template-rows: 30px 40px\"><div id=\"a\" style=\"width: 20px\"></div><div id=\"b\" style=\"width: 20px\"></div></div>",
        "body { margin: 0; }",
    );
    let a = layout_of(&dom, &layout, "a");
    let b = layout_of(&dom, &layout, "b");
    assert!(
        (a.abs_y - 0.0).abs() < 0.5,
        "item a should start at y=0, got {}",
        a.abs_y
    );
    assert!(
        (b.abs_y - 30.0).abs() < 0.5,
        "item b should start at y=30 (second row), got {}",
        b.abs_y
    );
}

#[test]
fn grid_fr_tracks_share_space() {
    // grid-template-columns: 1fr 1fr → 等分容器宽度（800px → 各 400px）。
    let (dom, layout) = full_pipeline(
        "<div id=\"grid\" style=\"display: grid; grid-template-columns: 1fr 1fr\"><div id=\"a\" style=\"height: 20px\"></div><div id=\"b\" style=\"height: 20px\"></div></div>",
        "body { margin: 0; }",
    );
    let a = layout_of(&dom, &layout, "a");
    let b = layout_of(&dom, &layout, "b");
    assert!(
        (a.width - 400.0).abs() < 1.0,
        "1fr of 800px = 400px, got {}",
        a.width
    );
    assert!(
        (b.abs_x - 400.0).abs() < 1.0,
        "item b starts at x=400 (second 1fr), got {}",
        b.abs_x
    );
}
