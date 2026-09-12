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

/// 构造 ident 关键字的 ComputedValue（如 `bold`）。
fn kw(s: &str) -> ComputedValue {
    ComputedValue::from_tokens(vec![ComponentValue::PreservedToken(Token::Ident(
        s.to_string(),
    ))])
}

/// 构造数字 ComputedValue（如 `line-height: 2`）。
fn num(val: f64) -> ComputedValue {
    ComputedValue::from_tokens(vec![ComponentValue::PreservedToken(Token::Number(
        Numeric {
            value: val,
            is_integer: false,
        },
    ))])
}

/// 构造「div[width: wpx + font 声明] > text」并 compute_layout，返回 text 布局。
///
/// `font_size` / `font_weight` 为 `Some` 时在容器上声明（text 节点继承）。
fn layout_text_with_font(
    width: f64,
    text: &str,
    font_size: Option<f64>,
    font_weight: Option<&str>,
) -> muskitty_layout::NodeLayout {
    let doc = Node::new_document();
    let container = Node::new_element_html("div", vec![], &doc);
    let text_node = Node::new_text(text, &doc);
    let text_addr = Rc::as_ptr(&text_node) as usize;
    append_child(&container, text_node).unwrap();

    let mut styles: HashMap<usize, ComputedStyle> = HashMap::new();
    let mut cs = ComputedStyle::new();
    cs.set("width", px(width));
    if let Some(size) = font_size {
        cs.set("font-size", px(size));
    }
    if let Some(weight) = font_weight {
        cs.set("font-weight", kw(weight));
    }
    styles.insert(Rc::as_ptr(&container) as usize, cs);

    let mut tree = build_layout_tree(&container, &styles);
    let result = compute_layout(&mut tree, 800.0, 600.0).expect("layout ok");
    *result.get(text_addr).expect("text node in layout")
}

#[test]
fn font_size_scales_measured_line_height() {
    // T-3：font-size 影响测量 —— 32px 的单行高明显大于 16px（line-height
    // 随字号缩放），宽度仍占满容器（Definite available width）。
    let small = layout_text_with_font(400.0, "Hello", Some(16.0), None);
    let large = layout_text_with_font(400.0, "Hello", Some(32.0), None);
    assert!(
        large.height > small.height * 1.5,
        "larger font-size should scale line height, large={} small={}",
        large.height,
        small.height
    );
    assert!(
        (large.width - 400.0).abs() < 1.0,
        "text width should fill container, got {}",
        large.width
    );
}

#[test]
fn font_weight_bold_keeps_container_width_single_line() {
    // T-3：font-weight: bold 不改变块级文本占满容器的宽度语义，
    // 单行高度与 normal 同字号一致（同行高）。
    let normal = layout_text_with_font(400.0, "Hello", Some(16.0), Some("normal"));
    let bold = layout_text_with_font(400.0, "Hello", Some(16.0), Some("bold"));
    assert!(
        (normal.width - 400.0).abs() < 1.0 && (bold.width - 400.0).abs() < 1.0,
        "both should fill container width, normal={} bold={}",
        normal.width,
        bold.width
    );
    assert!(
        (bold.height - normal.height).abs() < 2.0,
        "same font-size should keep line height, normal={} bold={}",
        normal.height,
        bold.height
    );
}

// —— LAY-3: measure 缓存 ——

/// 构造「div（auto 宽，可用宽度随视口）> text」并按给定视口布局。
fn layout_text_viewport(
    tree: &mut muskitty_layout::LayoutTree,
    text_addr: usize,
    vw: f32,
) -> (f32, f32) {
    let result = compute_layout(tree, vw, 600.0).expect("layout ok");
    let l = *result.get(text_addr).expect("text node in layout");
    (l.width, l.height)
}

/// LAY-3：同一节点以不同视口反复布局——缓存 key（可用宽度）变化时逐出
/// 重测，命中时复用；任一路径结果都必须与无缓存语义一致：更窄视口换行
/// 更多（高度更大），回到宽视口恢复原高度。
#[test]
fn measure_cache_hit_and_eviction_preserve_results() {
    let doc = Node::new_document();
    let container = Node::new_element_html("div", vec![], &doc);
    let text_node = Node::new_text("measure cache eviction sample text", &doc);
    let text_addr = Rc::as_ptr(&text_node) as usize;
    append_child(&container, text_node).unwrap();

    let styles: HashMap<usize, ComputedStyle> = HashMap::new();
    let mut tree = build_layout_tree(&container, &styles);

    // 宽视口：首测（冷缓存）。
    let (w1, h1) = layout_text_viewport(&mut tree, text_addr, 800.0);
    // 同视口再测：全命中缓存，结果一致。
    let (w2, h2) = layout_text_viewport(&mut tree, text_addr, 800.0);
    assert_eq!((w1, h1), (w2, h2), "cache hit must return identical size");

    // 窄视口：key 变化 → 逐出重测；文本换行更多，高度更大。
    let (_, h3) = layout_text_viewport(&mut tree, text_addr, 120.0);
    assert!(
        h3 > h1,
        "narrower viewport must wrap into more lines: narrow={h3} wide={h1}"
    );

    // 回宽视口：再次逐出重测，恢复宽视口高度。
    let (_, h4) = layout_text_viewport(&mut tree, text_addr, 800.0);
    assert!(
        (h1 - h4).abs() < f32::EPSILON,
        "back to wide viewport must restore height: first={h1} last={h4}"
    );
}

// —— M-3 batch 3: line-height / text-transform 测量 ——

/// 构造「div[width + 可选声明] > text」并返回 text 节点布局。
///
/// `decls` 为按序写入容器的 `(属性, 值)`（text 节点继承容器样式）。
fn layout_text_with_decls(
    width: f64,
    text: &str,
    decls: &[(&str, ComputedValue)],
) -> muskitty_layout::NodeLayout {
    let doc = Node::new_document();
    let container = Node::new_element_html("div", vec![], &doc);
    let text_node = Node::new_text(text, &doc);
    let text_addr = Rc::as_ptr(&text_node) as usize;
    append_child(&container, text_node).unwrap();

    let mut styles: HashMap<usize, ComputedStyle> = HashMap::new();
    let mut cs = ComputedStyle::new();
    cs.set("width", px(width));
    for (name, value) in decls {
        cs.set(*name, value.clone());
    }
    styles.insert(Rc::as_ptr(&container) as usize, cs);

    let mut tree = build_layout_tree(&container, &styles);
    let result = compute_layout(&mut tree, 800.0, 600.0).expect("layout ok");
    *result.get(text_addr).expect("text node in layout")
}

#[test]
fn line_height_px_sets_wrapped_line_spacing() {
    // 容器 100px 让长文本折成多行；line-height: 40px → 每行 40px 行距
    let text = "This is a long text that should wrap into multiple lines";
    let custom = layout_text_with_decls(100.0, text, &[("line-height", px(40.0))]);
    let default = layout_text_with_decls(100.0, text, &[]);
    assert!(
        custom.height > default.height,
        "40px line-height must exceed the 1.2×16px default: custom={} default={}",
        custom.height,
        default.height
    );
    // 多行高度应为行距的整数倍（同一份测量 → 单行高度的整数倍）
    let single = layout_text_with_decls(300.0, "Hi", &[("line-height", px(40.0))]);
    assert!(
        (single.height - 40.0).abs() < 1.0,
        "single line box should equal 40px line-height, got {}",
        single.height
    );
    let ratio = custom.height / single.height;
    assert!(
        (ratio - ratio.round()).abs() < 0.05,
        "wrapped height should be a whole number of 40px lines, got {}",
        custom.height
    );
}

#[test]
fn line_height_number_multiplies_font_size() {
    // line-height: 2 + font-size: 16px → 单行 32px（默认 1.2 → 19.2px）
    let lh2 = layout_text_with_decls(
        300.0,
        "Hi",
        &[("font-size", px(16.0)), ("line-height", num(2.0))],
    );
    let normal = layout_text_with_decls(300.0, "Hi", &[("font-size", px(16.0))]);
    assert!(
        (lh2.height - 32.0).abs() < 1.0,
        "line-height: 2 × 16px = 32px, got {}",
        lh2.height
    );
    assert!(
        (normal.height - 19.2).abs() < 1.0,
        "default normal line-height = 1.2 × 16px = 19.2px, got {}",
        normal.height
    );
}

#[test]
fn line_height_number_scales_with_font_size() {
    // 同一个倍数在 32px 字号下 → 64px（行高随字号缩放，不是固定值）
    let lh2_32 = layout_text_with_decls(
        300.0,
        "Hi",
        &[("font-size", px(32.0)), ("line-height", num(2.0))],
    );
    assert!(
        (lh2_32.height - 64.0).abs() < 1.0,
        "line-height: 2 × 32px = 64px, got {}",
        lh2_32.height
    );
}

#[test]
fn line_height_percentage_resolves_against_font_size() {
    // 150% × 16px = 24px（百分比经计算值阶段归一化为 px）
    let pct = layout_text_with_decls(
        300.0,
        "Hi",
        &[
            ("font-size", px(16.0)),
            (
                "line-height",
                ComputedValue::from_tokens(vec![ComponentValue::PreservedToken(
                    Token::Percentage(Numeric {
                        value: 150.0,
                        is_integer: false,
                    }),
                )]),
            ),
        ],
    );
    assert!(
        (pct.height - 24.0).abs() < 1.0,
        "150% × 16px = 24px, got {}",
        pct.height
    );
}

#[test]
fn text_transform_is_applied_before_measurement() {
    // 转换在布局前生效：`"abc" + uppercase` 与 `"ABC"`（无转换）必须测出
    // **完全相同**的排版结果——这比"大写更宽"之类的字体相关比较更稳健。
    let transformed = layout_text_with_decls(
        40.0,
        "abcdefghij klmnopqrst",
        &[("font-size", px(16.0)), ("text-transform", kw("uppercase"))],
    );
    let literal = layout_text_with_decls(40.0, "ABCDEFGHIJ KLMNOPQRST", &[("font-size", px(16.0))]);
    assert!(
        (transformed.height - literal.height).abs() < f32::EPSILON,
        "transformed text must measure identically to the literal uppercase text: \
         transformed={} literal={}",
        transformed.height,
        literal.height
    );

    // capitalize 同理：首字母大写后与字面量一致
    let capitalized = layout_text_with_decls(
        40.0,
        "hello world",
        &[
            ("font-size", px(16.0)),
            ("text-transform", kw("capitalize")),
        ],
    );
    let literal_cap = layout_text_with_decls(40.0, "Hello World", &[("font-size", px(16.0))]);
    assert!(
        (capitalized.height - literal_cap.height).abs() < f32::EPSILON,
        "capitalize must measure identically to the literal capitalized text: \
         capitalized={} literal={}",
        capitalized.height,
        literal_cap.height
    );
}

#[test]
fn text_transform_none_measures_source_text() {
    // 对照组：无转换时 "hello world" 与 "HELLO WORLD" 在大写更宽的字体下
    // 行数不会更少（宽度不同 → 测量结果随文本而变，证明上面的相等不是恒等）
    let lower = layout_text_with_decls(40.0, "hello world", &[("font-size", px(16.0))]);
    let upper = layout_text_with_decls(40.0, "HELLO WORLD", &[("font-size", px(16.0))]);
    assert!(
        upper.height >= lower.height,
        "uppercase glyphs are at least as wide, so never fewer lines: upper={} lower={}",
        upper.height,
        lower.height
    );
}
