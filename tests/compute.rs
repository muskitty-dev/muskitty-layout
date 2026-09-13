//! compute_layout 布局计算结果测试。
//!
//! 覆盖：空树、固定尺寸元素、Block 填满视口宽度、Flex 行/列布局、
//! padding 偏移子元素、justify-content:space-between 间距分配。

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use muskitty_cascade::{ComputedStyle, ComputedValue};
use muskitty_css::parser::ComponentValue;
use muskitty_css::tokenizer::{Numeric, Token};
use muskitty_dom::{append_child, Node};
use muskitty_layout::{build_layout_tree, compute_layout};

/// 创建 Document + 一个指定 tag 的 Element。
fn make_element(tag: &str, doc: &Rc<RefCell<Node>>) -> Rc<RefCell<Node>> {
    Node::new_element_html(tag, vec![], doc)
}

/// 构造 `Npx` 的 ComputedValue。
fn px(val: f64) -> ComputedValue {
    ComputedValue::from_tokens(vec![ComponentValue::PreservedToken(Token::Dimension(
        Numeric {
            value: val,
            is_integer: false,
            has_sign: false,
        },
        "px".to_string(),
    ))])
}

/// 构造关键字 ComputedValue。
fn kw(s: &str) -> ComputedValue {
    ComputedValue::from_keyword(s)
}

/// 构造纯数字的 ComputedValue（用于 flex-grow/flex-shrink）。
fn num(val: f64) -> ComputedValue {
    ComputedValue::from_tokens(vec![ComponentValue::PreservedToken(Token::Number(
        Numeric {
            value: val,
            is_integer: false,
            has_sign: false,
        },
    ))])
}

/// 浮点近似相等比较（容差 eps）。
fn approx_eq(a: f32, b: f32, eps: f32) -> bool {
    (a - b).abs() < eps
}

/// 默认容差 0.01 的浮点近似相等断言。
fn assert_approx(actual: f32, expected: f32, label: &str) {
    assert!(
        approx_eq(actual, expected, 0.01),
        "{label}: 期望 {expected}, 实际 {actual}"
    );
}

#[test]
fn empty_tree_returns_empty_result() {
    // Document 节点（非 Element）→ 空布局树 → 空结果
    let doc = Node::new_document();
    let styles: HashMap<usize, ComputedStyle> = HashMap::new();
    let mut tree = build_layout_tree(&doc, &styles);

    let result = compute_layout(&mut tree, 800.0, 600.0).expect("layout should succeed");

    assert!(result.nodes.is_empty(), "空 DOM 树应产生空布局结果");
}

#[test]
fn single_element_fixed_size() {
    // div[width:200px, height:100px]
    let doc = Node::new_document();
    let root = make_element("div", &doc);

    let mut root_style = ComputedStyle::new();
    root_style.set("width", px(200.0));
    root_style.set("height", px(100.0));

    let mut styles: HashMap<usize, ComputedStyle> = HashMap::new();
    styles.insert(Rc::as_ptr(&root) as usize, root_style);

    let mut tree = build_layout_tree(&root, &styles);
    let result = compute_layout(&mut tree, 800.0, 600.0).expect("layout should succeed");

    let layout = result
        .get(Rc::as_ptr(&root) as usize)
        .expect("根节点应有布局结果");

    assert_approx(layout.width, 200.0, "根节点宽度");
    assert_approx(layout.height, 100.0, "根节点高度");
    assert_approx(layout.x, 0.0, "根节点 x 坐标");
    assert_approx(layout.y, 0.0, "根节点 y 坐标");
}

// —— M-3 batch 2: border 参与盒模型（CSS Box Model L3 §2-§3）——

/// 四向 border 全设为 `width px` + `style`。
fn set_border_all(cs: &mut ComputedStyle, width: f64, style_kw: &str) {
    for side in ["top", "right", "bottom", "left"] {
        cs.set(format!("border-{side}-style"), kw(style_kw));
        cs.set(format!("border-{side}-width"), px(width));
    }
}

/// 单元素布局（根节点尺寸），可指定 box-sizing 与边框。
fn layout_root_with_border(
    border: Option<(f64, &str)>,
    box_sizing: Option<&str>,
) -> muskitty_layout::NodeLayout {
    let doc = Node::new_document();
    let root = make_element("div", &doc);
    let mut root_style = ComputedStyle::new();
    root_style.set("width", px(100.0));
    root_style.set("height", px(50.0));
    if let Some((w, s)) = border {
        set_border_all(&mut root_style, w, s);
    }
    if let Some(bs) = box_sizing {
        root_style.set("box-sizing", kw(bs));
    }
    let mut styles: HashMap<usize, ComputedStyle> = HashMap::new();
    styles.insert(Rc::as_ptr(&root) as usize, root_style);
    let mut tree = build_layout_tree(&root, &styles);
    let result = compute_layout(&mut tree, 800.0, 600.0).expect("layout should succeed");
    *result
        .get(Rc::as_ptr(&root) as usize)
        .expect("根节点应有布局结果")
}

#[test]
fn border_adds_to_content_box_size() {
    // content-box（初始值）：border box = content + border×2
    let layout = layout_root_with_border(Some((10.0, "solid")), None);
    assert_approx(layout.width, 120.0, "width:100 + 10px border×2");
    assert_approx(layout.height, 70.0, "height:50 + 10px border×2");
}

#[test]
fn border_with_border_box_sizing_stays_declared_size() {
    // border-box：声明尺寸含边框，边框不额外增大盒子
    let layout = layout_root_with_border(Some((10.0, "solid")), Some("border-box"));
    assert_approx(layout.width, 100.0, "border-box 尺寸含边框");
    assert_approx(layout.height, 50.0, "border-box 尺寸含边框");
}

#[test]
fn border_style_none_takes_no_space() {
    // §4.1: style none → used width 0，即使声明了 10px 宽度
    let layout = layout_root_with_border(Some((10.0, "none")), None);
    assert_approx(layout.width, 100.0, "none 不占空间");
    assert_approx(layout.height, 50.0, "none 不占空间");
}

#[test]
fn directional_border_offsets_child_only_on_that_side() {
    // div[display:flex, width:200px, height:100px, border-left:20px, border-top:10px]
    //   > span[50x50]
    // 子元素原点相对父元素 border-box：偏移 = border + padding
    let doc = Node::new_document();
    let root = make_element("div", &doc);
    let child = make_element("span", &doc);
    append_child(&root, Rc::clone(&child)).unwrap();

    let mut root_style = ComputedStyle::new();
    root_style.set("display", kw("flex"));
    root_style.set("width", px(200.0));
    root_style.set("height", px(100.0));
    root_style.set("border-left-style", kw("solid"));
    root_style.set("border-left-width", px(20.0));
    root_style.set("border-top-style", kw("solid"));
    root_style.set("border-top-width", px(10.0));

    let mut child_style = ComputedStyle::new();
    child_style.set("width", px(50.0));
    child_style.set("height", px(50.0));

    let mut styles: HashMap<usize, ComputedStyle> = HashMap::new();
    styles.insert(Rc::as_ptr(&root) as usize, root_style);
    styles.insert(Rc::as_ptr(&child) as usize, child_style);

    let mut tree = build_layout_tree(&root, &styles);
    let result = compute_layout(&mut tree, 800.0, 600.0).expect("layout should succeed");

    let c = result
        .get(Rc::as_ptr(&child) as usize)
        .expect("child 应有布局结果");
    assert_approx(c.x, 20.0, "border-left:20px → child.x");
    assert_approx(c.y, 10.0, "border-top:10px → child.y");
}

#[test]
fn block_element_fills_viewport_width() {
    // div（无 width → auto → Block 元素填满可用宽度）
    let doc = Node::new_document();
    let root = make_element("div", &doc);

    let styles: HashMap<usize, ComputedStyle> = HashMap::new();
    let mut tree = build_layout_tree(&root, &styles);
    let result = compute_layout(&mut tree, 800.0, 600.0).expect("layout should succeed");

    let layout = result
        .get(Rc::as_ptr(&root) as usize)
        .expect("根节点应有布局结果");

    assert_approx(layout.width, 800.0, "Block auto 宽度应填满视口");
}

#[test]
fn flex_row_children_horizontal_layout() {
    // div[display:flex, width:200px, height:100px]
    //   > span[width:50px, height:20px] + span[width:50px, height:20px]
    let doc = Node::new_document();
    let root = make_element("div", &doc);
    let child1 = make_element("span", &doc);
    let child2 = make_element("span", &doc);
    append_child(&root, Rc::clone(&child1)).unwrap();
    append_child(&root, Rc::clone(&child2)).unwrap();

    let mut root_style = ComputedStyle::new();
    root_style.set("display", kw("flex"));
    root_style.set("width", px(200.0));
    root_style.set("height", px(100.0));

    let mut child_style = ComputedStyle::new();
    child_style.set("width", px(50.0));
    child_style.set("height", px(20.0));

    let mut styles: HashMap<usize, ComputedStyle> = HashMap::new();
    styles.insert(Rc::as_ptr(&root) as usize, root_style);
    styles.insert(Rc::as_ptr(&child1) as usize, child_style.clone());
    styles.insert(Rc::as_ptr(&child2) as usize, child_style);

    let mut tree = build_layout_tree(&root, &styles);
    let result = compute_layout(&mut tree, 800.0, 600.0).expect("layout should succeed");

    let c1 = result
        .get(Rc::as_ptr(&child1) as usize)
        .expect("child1 应有布局结果");
    let c2 = result
        .get(Rc::as_ptr(&child2) as usize)
        .expect("child2 应有布局结果");

    // flex-direction: row（默认）→ 子元素水平排列
    assert_approx(c1.x, 0.0, "第一个子元素 x");
    assert_approx(c2.x, 50.0, "第二个子元素 x（紧接第一个子元素之后）");
    assert_approx(c1.width, 50.0, "child1 宽度");
    assert_approx(c2.width, 50.0, "child2 宽度");
}

#[test]
fn flex_column_children_vertical_layout() {
    // div[display:flex, flex-direction:column, width:100px, height:200px]
    //   > span[height:50px] + span[height:50px]
    let doc = Node::new_document();
    let root = make_element("div", &doc);
    let child1 = make_element("span", &doc);
    let child2 = make_element("span", &doc);
    append_child(&root, Rc::clone(&child1)).unwrap();
    append_child(&root, Rc::clone(&child2)).unwrap();

    let mut root_style = ComputedStyle::new();
    root_style.set("display", kw("flex"));
    root_style.set("flex-direction", kw("column"));
    root_style.set("width", px(100.0));
    root_style.set("height", px(200.0));

    let mut child_style = ComputedStyle::new();
    child_style.set("height", px(50.0));

    let mut styles: HashMap<usize, ComputedStyle> = HashMap::new();
    styles.insert(Rc::as_ptr(&root) as usize, root_style);
    styles.insert(Rc::as_ptr(&child1) as usize, child_style.clone());
    styles.insert(Rc::as_ptr(&child2) as usize, child_style);

    let mut tree = build_layout_tree(&root, &styles);
    let result = compute_layout(&mut tree, 800.0, 600.0).expect("layout should succeed");

    let c1 = result
        .get(Rc::as_ptr(&child1) as usize)
        .expect("child1 应有布局结果");
    let c2 = result
        .get(Rc::as_ptr(&child2) as usize)
        .expect("child2 应有布局结果");

    // flex-direction: column → 子元素垂直排列
    assert_approx(c1.y, 0.0, "第一个子元素 y");
    assert_approx(c2.y, 50.0, "第二个子元素 y（紧接第一个子元素之下）");
}

#[test]
fn padding_offsets_child_position() {
    // div[display:flex, width:200px, height:100px, padding-left:20px, padding-top:10px]
    //   > span[width:50px, height:50px]
    let doc = Node::new_document();
    let root = make_element("div", &doc);
    let child = make_element("span", &doc);
    append_child(&root, Rc::clone(&child)).unwrap();

    let mut root_style = ComputedStyle::new();
    root_style.set("display", kw("flex"));
    root_style.set("width", px(200.0));
    root_style.set("height", px(100.0));
    root_style.set("padding-left", px(20.0));
    root_style.set("padding-top", px(10.0));

    let mut child_style = ComputedStyle::new();
    child_style.set("width", px(50.0));
    child_style.set("height", px(50.0));

    let mut styles: HashMap<usize, ComputedStyle> = HashMap::new();
    styles.insert(Rc::as_ptr(&root) as usize, root_style);
    styles.insert(Rc::as_ptr(&child) as usize, child_style);

    let mut tree = build_layout_tree(&root, &styles);
    let result = compute_layout(&mut tree, 800.0, 600.0).expect("layout should succeed");

    let c = result
        .get(Rc::as_ptr(&child) as usize)
        .expect("child 应有布局结果");

    // 子元素原点应偏移 padding（location 相对父元素 border-box 原点）
    assert_approx(c.x, 20.0, "padding-left:20px → child.x");
    assert_approx(c.y, 10.0, "padding-top:10px → child.y");
}

#[test]
fn flex_justify_content_space_between() {
    // div[display:flex, width:300px, height:100px, justify-content:space-between]
    //   > span[50px] × 3
    // 3 个 50px 子元素 = 150px，剩余 150px / 2 间距 = 75px
    // → child1.x=0, child2.x=125, child3.x=250
    let doc = Node::new_document();
    let root = make_element("div", &doc);
    let child1 = make_element("span", &doc);
    let child2 = make_element("span", &doc);
    let child3 = make_element("span", &doc);
    append_child(&root, Rc::clone(&child1)).unwrap();
    append_child(&root, Rc::clone(&child2)).unwrap();
    append_child(&root, Rc::clone(&child3)).unwrap();

    let mut root_style = ComputedStyle::new();
    root_style.set("display", kw("flex"));
    root_style.set("width", px(300.0));
    root_style.set("height", px(100.0));
    root_style.set("justify-content", kw("space-between"));

    let mut child_style = ComputedStyle::new();
    child_style.set("width", px(50.0));
    child_style.set("height", px(20.0));

    let mut styles: HashMap<usize, ComputedStyle> = HashMap::new();
    styles.insert(Rc::as_ptr(&root) as usize, root_style);
    styles.insert(Rc::as_ptr(&child1) as usize, child_style.clone());
    styles.insert(Rc::as_ptr(&child2) as usize, child_style.clone());
    styles.insert(Rc::as_ptr(&child3) as usize, child_style);

    let mut tree = build_layout_tree(&root, &styles);
    let result = compute_layout(&mut tree, 800.0, 600.0).expect("layout should succeed");

    let c1 = result
        .get(Rc::as_ptr(&child1) as usize)
        .expect("child1 应有布局结果");
    let c2 = result
        .get(Rc::as_ptr(&child2) as usize)
        .expect("child2 应有布局结果");
    let c3 = result
        .get(Rc::as_ptr(&child3) as usize)
        .expect("child3 应有布局结果");

    // space-between: 首元素贴左、尾元素贴右、中间均匀分布
    assert_approx(c1.x, 0.0, "space-between 首元素 x");
    assert_approx(c2.x, 125.0, "space-between 中间元素 x（50 + 75）");
    assert_approx(c3.x, 250.0, "space-between 尾元素 x（300 - 50）");
}

#[test]
fn flex_grow_distributes_free_space() {
    // div[display:flex, width:350px, height:100px]
    //   > span[width:100px, flex-grow:1] + span[width:100px, flex-grow:2]
    // 剩余空间 150px，按 1:2 分配 → child1 +50px = 150px, child2 +100px = 200px
    let doc = Node::new_document();
    let root = make_element("div", &doc);
    let child1 = make_element("span", &doc);
    let child2 = make_element("span", &doc);
    append_child(&root, Rc::clone(&child1)).unwrap();
    append_child(&root, Rc::clone(&child2)).unwrap();

    let mut root_style = ComputedStyle::new();
    root_style.set("display", kw("flex"));
    root_style.set("width", px(350.0));
    root_style.set("height", px(100.0));

    let mut c1_style = ComputedStyle::new();
    c1_style.set("width", px(100.0));
    c1_style.set("flex-grow", num(1.0));

    let mut c2_style = ComputedStyle::new();
    c2_style.set("width", px(100.0));
    c2_style.set("flex-grow", num(2.0));

    let mut styles: HashMap<usize, ComputedStyle> = HashMap::new();
    styles.insert(Rc::as_ptr(&root) as usize, root_style);
    styles.insert(Rc::as_ptr(&child1) as usize, c1_style);
    styles.insert(Rc::as_ptr(&child2) as usize, c2_style);

    let mut tree = build_layout_tree(&root, &styles);
    let result = compute_layout(&mut tree, 800.0, 600.0).expect("layout should succeed");

    let c1 = result
        .get(Rc::as_ptr(&child1) as usize)
        .expect("child1 应有布局结果");
    let c2 = result
        .get(Rc::as_ptr(&child2) as usize)
        .expect("child2 应有布局结果");

    // 剩余空间 150px，flex-grow 1:2 → child1 +50px, child2 +100px
    assert_approx(c1.width, 150.0, "flex-grow:1 → child1 宽度 100+50");
    assert_approx(c2.width, 200.0, "flex-grow:2 → child2 宽度 100+100");
    assert_approx(c2.x, 150.0, "child2 x = child1 宽度");
}

/// 纯空白文本节点（源码换行/缩进）不产生布局盒：`<p>汉字</p>` 与
/// 前后带纯空白兄弟节点的版本，文本盒位置一致（块级边界空白按 CSS
/// 应为零尺寸，否则每处源码换行会把后续内容下推 2 行文本高）。
#[test]
fn whitespace_only_text_node_takes_no_space() {
    use muskitty_dom::{append_child, Node};

    fn layout_p_with(ws_before: bool) -> f32 {
        let doc = Node::new_document();
        if ws_before {
            // 模拟源码换行+缩进：换行符 + 空格组成的纯空白文本。
            let ws = Node::new_text("\n        ", &doc);
            drop(append_child(&doc, ws));
        }
        let p = make_element("p", &doc);
        let text = Node::new_text("汉字测试", &doc);
        drop(append_child(&p, text));
        drop(append_child(&doc, p));
        let styles: HashMap<usize, ComputedStyle> = HashMap::new();
        let mut tree = build_layout_tree(&doc, &styles);
        let layout = compute_layout(&mut tree, 900.0, 600.0).expect("layout");
        layout
            .nodes
            .values()
            .map(|n| n.y)
            .fold(f32::NAN, |acc, y| if acc.is_nan() { y } else { acc.min(y) })
    }

    assert_eq!(layout_p_with(false), 0.0, "no-whitespace p starts at 0");
    assert_eq!(
        layout_p_with(true),
        0.0,
        "whitespace-only sibling must not push p down"
    );
}
