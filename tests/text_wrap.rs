//! T-3 换行集成测试：text 节点按容器宽度换行（taffy measure function）。

use std::collections::HashMap;
use std::rc::Rc;

use muskitty_cascade::{ComputedStyle, ComputedValue};
use muskitty_css::parser::ComponentValue;
use muskitty_css::tokenizer::{Numeric, Token};
use muskitty_dom::{append_child, Node};
use muskitty_layout::{build_layout_tree, compute_layout};

/// 构造 `Npx` 的 ComputedValue。
fn px(val: f64) -> ComputedValue {
    ComputedValue::from_tokens(vec![ComponentValue::PreservedToken(Token::Dimension(
        Numeric {
            value: val,
            is_integer: false,
        },
        "px".to_string(),
    ))])
}

/// 构造「div[width: wpx] > text」并 compute_layout，返回 text 节点布局。
fn layout_text_in(width: f64, text: &str) -> muskitty_layout::NodeLayout {
    let doc = Node::new_document();
    let container = Node::new_element_html("div", vec![], &doc);
    let text_node = Node::new_text(text, &doc);
    let text_addr = Rc::as_ptr(&text_node) as usize;
    append_child(&container, text_node).unwrap();

    let mut styles: HashMap<usize, ComputedStyle> = HashMap::new();
    let mut cs = ComputedStyle::new();
    cs.set("width", px(width));
    styles.insert(Rc::as_ptr(&container) as usize, cs);

    let mut tree = build_layout_tree(&container, &styles);
    let result = compute_layout(&mut tree, 800.0, 600.0).expect("layout ok");
    *result.get(text_addr).expect("text node in layout")
}

#[test]
fn long_text_wraps_in_narrow_container() {
    // 长文本在 100px 容器里换行：高度明显大于单行文本（多行）。
    let long = layout_text_in(
        100.0,
        "This is a long text that should wrap into multiple lines",
    );
    let single = layout_text_in(100.0, "Hi");
    assert!(
        long.height > single.height * 2.0,
        "long text should wrap to multiple lines, long={} single={}",
        long.height,
        single.height
    );
    // 换行时宽度 = 容器宽（占满行）。
    assert!(
        (long.width - 100.0).abs() < 1.0,
        "text width should fill container, got {}",
        long.width
    );
}

#[test]
fn short_text_stays_single_line() {
    // 短文本单行：高度是字体的单行高（合理范围 10..100 px）。
    let l = layout_text_in(200.0, "Hi");
    assert!(
        (10.0..100.0).contains(&l.height),
        "single line height should be reasonable, got {}",
        l.height
    );
}

#[test]
fn wide_container_no_wrap() {
    // 宽容器（800px）里的中等文本：高度 ≈ 单行高（与窄容器短文本一致）。
    let wide = layout_text_in(800.0, "A short-ish sentence that fits.");
    let single = layout_text_in(200.0, "Hi");
    assert!(
        (wide.height - single.height).abs() < 1.0,
        "wide container text should stay single line, wide={} single={}",
        wide.height,
        single.height
    );
}
