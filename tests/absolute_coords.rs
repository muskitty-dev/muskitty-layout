//! NodeLayout 绝对坐标（abs_x/abs_y）测试（P2-19 / PERF-12）。
//!
//! [`compute_layout`] 沿 taffy 树自根累加偏移，把每个盒映射到画布坐标系
//! （视口左上角原点）。`display: contents` / 非渲染标签 splice 后 DOM 祖先链
//! ≠ taffy 父链，只有 taffy 树能给出权威绝对坐标；Renderer 应直接读
//! `abs_x` / `abs_y`，不得再沿 DOM 祖先累加（否则未来 `position: absolute`
//! 引入时双重计数）。

use muskitty_cascade::{compute_styles, StyleTreeOptions};
use muskitty_css::parse_stylesheet;
use muskitty_cssom::{from_stylesheet, Origin};
use muskitty_dom::{Node, NodeKind};
use muskitty_layout::{build_layout_tree, compute_layout};
use std::cell::RefCell;
use std::rc::Rc;

struct PipelineResult {
    dom: Rc<RefCell<Node>>,
    layout: muskitty_layout::LayoutResult,
}

fn full_pipeline(html: &str, css: &str, vw: f32, vh: f32) -> PipelineResult {
    let dom = muskitty_html5_parser::parse(html);
    let parsed = parse_stylesheet(css);
    let sheet = {
        let mut s = from_stylesheet(&parsed);
        s.origin = Origin::Author;
        s
    };
    let styles = compute_styles(&dom, &[sheet], &StyleTreeOptions::default());
    let mut tree = build_layout_tree(&dom, &styles);
    let layout = compute_layout(&mut tree, vw, vh).expect("layout should succeed");
    PipelineResult { dom, layout }
}

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

/// 收集所有 div 的地址（DOM 先序）。
fn div_addrs(node: &Rc<RefCell<Node>>) -> Vec<usize> {
    collect_all_elements(node)
        .into_iter()
        .filter(|(_, n)| n.eq_ignore_ascii_case("div"))
        .map(|(a, _)| a)
        .collect()
}

#[test]
fn abs_coords_accumulate_padding_and_margin() {
    // div(padding-left:10px; padding-top:10px) > div(width:50;height:50;margin-left:5px)
    // 内层盒 abs = (padding-left + margin-left, padding-top) = (15, 10)。
    // 注：registry 未注册 `padding` 简写（P2-7 只注册长属性），测试用长属性。
    let result = full_pipeline(
        "<div style='padding-left: 10px; padding-top: 10px'><div style='width: 50px; height: 50px; margin-left: 5px'></div></div>",
        "",
        800.0,
        600.0,
    );
    let addrs = div_addrs(&result.dom);
    assert!(
        addrs.len() >= 2,
        "expected outer+inner div, got {}",
        addrs.len()
    );
    let inner_addr = addrs[1];
    let inner = result.layout.get(inner_addr).expect("inner has layout");
    assert!(
        (inner.abs_x - 15.0).abs() < 1.0,
        "inner abs_x should be ~15 (padding-left 10 + margin-left 5), got {}",
        inner.abs_x
    );
    assert!(
        (inner.abs_y - 10.0).abs() < 1.0,
        "inner abs_y should be ~10 (padding-top), got {}",
        inner.abs_y
    );
}

#[test]
fn abs_coords_through_contents_splice() {
    // div(display:flex; padding-left:10px; padding-top:10px) > span(display:contents) > div
    // span 不生成盒，inner splice 为 div(flex) 的直接子盒；
    // inner abs = (padding-left, padding-top) = (10, 10)。
    // DOM 祖先链 div>span>div ≠ taffy 父链 div>inner，只能沿 taffy 树累加。
    let result = full_pipeline(
        "<div style='display: flex; padding-left: 10px; padding-top: 10px'><span style='display: contents'><div style='width: 50px; height: 50px'></div></span></div>",
        "",
        800.0,
        600.0,
    );
    let addrs = div_addrs(&result.dom);
    assert!(
        addrs.len() >= 2,
        "expected flex div + inner div, got {}",
        addrs.len()
    );
    let inner_addr = addrs[1];
    let inner = result.layout.get(inner_addr).expect("inner has layout");
    assert!(
        (inner.abs_x - 10.0).abs() < 1.0,
        "inner abs_x should be ~10 (flex padding-left), got {}",
        inner.abs_x
    );
    assert!(
        (inner.abs_y - 10.0).abs() < 1.0,
        "inner abs_y should be ~10 (flex padding-top), got {}",
        inner.abs_y
    );
}

#[test]
fn abs_coords_siblings_share_parent_offset() {
    // div(display:flex; padding-left:10px; padding-top:10px) 内两个同层子盒，
    // 第二盒 x 在第一盒之后；两盒 abs_y 均为 padding-top=10。
    let result = full_pipeline(
        "<div style='display: flex; padding-left: 10px; padding-top: 10px'>\
           <div style='width: 40px; height: 40px'></div>\
           <div style='width: 60px; height: 40px'></div>\
         </div>",
        "",
        800.0,
        600.0,
    );
    let addrs = div_addrs(&result.dom);
    assert!(
        addrs.len() >= 3,
        "expected flex + 2 children, got {}",
        addrs.len()
    );
    let a = result.layout.get(addrs[1]).expect("child1 layout");
    let b = result.layout.get(addrs[2]).expect("child2 layout");
    assert!(
        (a.abs_y - 10.0).abs() < 1.0,
        "child1 abs_y ~10, got {}",
        a.abs_y
    );
    assert!(
        (b.abs_y - 10.0).abs() < 1.0,
        "child2 abs_y ~10, got {}",
        b.abs_y
    );
    assert!(
        (b.abs_x - (a.abs_x + a.width)).abs() < 1.0,
        "child2 abs_x ({}) should be ~child1.abs_x ({}) + child1.width ({})",
        b.abs_x,
        a.abs_x,
        a.width
    );
}
