//! ComputedStyle → taffy::Style 映射测试。
//!
//! 覆盖：display / 尺寸 / margin / padding / flexbox 属性 / box-sizing / gap。

use muskitty_cascade::{ComputedStyle, ComputedValue};
use muskitty_css::parser::ComponentValue;
use muskitty_css::tokenizer::{Numeric, Token};
use muskitty_layout::style_map::map_style;
use taffy::prelude::*;
use taffy::style::{BoxSizing, Dimension, Display, FlexDirection, FlexWrap, LengthPercentage};

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

/// 构造 `N%` 的 ComputedValue。
fn pct(val: f64) -> ComputedValue {
    ComputedValue::from_tokens(vec![ComponentValue::PreservedToken(Token::Percentage(
        Numeric {
            value: val,
            is_integer: false,
        },
    ))])
}

/// 构造纯数字的 ComputedValue（用于 flex-grow/flex-shrink）。
fn num(val: f64) -> ComputedValue {
    ComputedValue::from_tokens(vec![ComponentValue::PreservedToken(Token::Number(
        Numeric {
            value: val,
            is_integer: false,
        },
    ))])
}

fn kw(s: &str) -> ComputedValue {
    ComputedValue::from_keyword(s)
}

/// 将内部 component values 包进 `calc(...)` Function（P1-9）。
fn calc(inner: Vec<ComponentValue>) -> ComputedValue {
    ComputedValue::from_tokens(vec![ComponentValue::Function(
        muskitty_css::parser::Function {
            name: "calc".to_string(),
            value: inner,
        },
    )])
}

/// 构造 `calc(<Npx>)` 的 Function ComputedValue。
fn calc_px(val: f64) -> ComputedValue {
    calc(vec![ComponentValue::PreservedToken(Token::Dimension(
        Numeric {
            value: val,
            is_integer: false,
        },
        "px".to_string(),
    ))])
}

/// 构造 `calc(<N%>)` 的 Function ComputedValue。
fn calc_pct(val: f64) -> ComputedValue {
    calc(vec![ComponentValue::PreservedToken(Token::Percentage(
        Numeric {
            value: val,
            is_integer: false,
        },
    ))])
}

// —— display ——

#[test]
fn display_block_maps_to_block() {
    let mut cs = ComputedStyle::new();
    cs.set("display", kw("block"));
    let style = map_style(Some(&cs));
    assert_eq!(style.display, Display::Block);
}

#[test]
fn display_flex_maps_to_flex() {
    let mut cs = ComputedStyle::new();
    cs.set("display", kw("flex"));
    let style = map_style(Some(&cs));
    assert_eq!(style.display, Display::Flex);
}

#[test]
fn display_grid_maps_to_grid() {
    let mut cs = ComputedStyle::new();
    cs.set("display", kw("grid"));
    let style = map_style(Some(&cs));
    assert_eq!(style.display, Display::Grid);
}

#[test]
fn display_none_maps_to_none() {
    let mut cs = ComputedStyle::new();
    cs.set("display", kw("none"));
    let style = map_style(Some(&cs));
    assert_eq!(style.display, Display::None);
}

#[test]
fn display_inline_flex_maps_to_flex() {
    // CSS Display §2.8: inline-flex = inline-level + flex container.
    // taffy 只支持 flex container 部分，映射为 Flex。
    let mut cs = ComputedStyle::new();
    cs.set("display", kw("inline-flex"));
    let style = map_style(Some(&cs));
    assert_eq!(style.display, Display::Flex);
}

#[test]
fn display_inline_grid_maps_to_grid() {
    // CSS Display §2.8: inline-grid = inline-level + grid container.
    // taffy 只支持 grid container 部分，映射为 Grid。
    let mut cs = ComputedStyle::new();
    cs.set("display", kw("inline-grid"));
    let style = map_style(Some(&cs));
    assert_eq!(style.display, Display::Grid);
}

#[test]
fn display_inline_maps_to_block() {
    // CSS Display §2.4: display: inline 是初始值，生成 inline-level box.
    // taffy 0.12 无 inline layout 支持，作为 workaround 映射为 Block.
    let mut cs = ComputedStyle::new();
    cs.set("display", kw("inline"));
    let style = map_style(Some(&cs));
    assert_eq!(style.display, Display::Block);
}

#[test]
fn display_inline_block_maps_to_block() {
    // CSS Display §2.4: inline-block = inline-level + block container.
    // taffy 无 inline-level 支持，映射为 Block.
    let mut cs = ComputedStyle::new();
    cs.set("display", kw("inline-block"));
    let style = map_style(Some(&cs));
    assert_eq!(style.display, Display::Block);
}

#[test]
fn no_display_defaults_to_block() {
    let cs = ComputedStyle::new();
    let style = map_style(Some(&cs));
    assert_eq!(style.display, Display::Block);
}

#[test]
fn no_computed_style_defaults_to_block() {
    let style = map_style(None);
    assert_eq!(style.display, Display::Block);
}

#[test]
fn keywords_match_case_insensitively() {
    // PERF-10: get_keyword 返回借用 &str，比较走 kw_eq（eq_ignore_ascii_case）。
    // 大写关键字与全小写等价，行为与重构前 to_ascii_lowercase 一致。
    let mut cs = ComputedStyle::new();
    cs.set("display", kw("FLEX"));
    cs.set("box-sizing", kw("BORDER-BOX"));
    cs.set("flex-direction", kw("COLUMN"));
    cs.set("flex-wrap", kw("WRAP"));
    cs.set("justify-content", kw("CENTER"));
    cs.set("align-items", kw("STRETCH"));
    let style = map_style(Some(&cs));
    assert_eq!(style.display, Display::Flex);
    assert_eq!(style.box_sizing, BoxSizing::BorderBox);
    assert_eq!(style.flex_direction, FlexDirection::Column);
    assert_eq!(style.flex_wrap, FlexWrap::Wrap);
    assert_eq!(style.justify_content, Some(JustifyContent::CENTER));
    assert_eq!(style.align_items, Some(AlignItems::STRETCH));
}

// —— width / height ——

#[test]
fn width_px_maps_to_length() {
    let mut cs = ComputedStyle::new();
    cs.set("width", px(200.0));
    let style = map_style(Some(&cs));
    assert_eq!(style.size.width, Dimension::length(200.0));
}

#[test]
fn width_auto_maps_to_auto() {
    let mut cs = ComputedStyle::new();
    cs.set("width", kw("auto"));
    let style = map_style(Some(&cs));
    assert!(
        style.size.width.is_auto(),
        "width: auto 应映射为 Dimension::AUTO"
    );
}

#[test]
fn width_percentage_maps_to_percent() {
    let mut cs = ComputedStyle::new();
    cs.set("width", pct(50.0));
    let style = map_style(Some(&cs));
    assert_eq!(style.size.width, Dimension::percent(0.5));
}

#[test]
fn height_px_maps_to_length() {
    let mut cs = ComputedStyle::new();
    cs.set("height", px(100.0));
    let style = map_style(Some(&cs));
    assert_eq!(style.size.height, Dimension::length(100.0));
}

#[test]
fn min_max_width_mapped() {
    let mut cs = ComputedStyle::new();
    cs.set("min-width", px(50.0));
    cs.set("max-width", px(300.0));
    let style = map_style(Some(&cs));
    assert_eq!(style.min_size.width, Dimension::length(50.0));
    assert_eq!(style.max_size.width, Dimension::length(300.0));
}

#[test]
fn width_zero_number_maps_to_length_0() {
    // P1-8: 裸 0 是合法 <length>（CSS Values L4 §5.1）。
    // 修复前 width: 0 → AUTO（填满父宽），应映射为 length(0)。
    let mut cs = ComputedStyle::new();
    cs.set("width", num(0.0));
    let style = map_style(Some(&cs));
    assert_eq!(style.size.width, Dimension::length(0.0));
}

#[test]
fn min_width_zero_maps_to_length_0() {
    // P1-8: min-width: 0 是 flex 收缩最常用的修复，不能落到 AUTO。
    let mut cs = ComputedStyle::new();
    cs.set("min-width", num(0.0));
    let style = map_style(Some(&cs));
    assert_eq!(style.min_size.width, Dimension::length(0.0));
}

#[test]
fn flex_basis_zero_maps_to_length_0() {
    // P1-8: flex-basis: 0 → length(0)，而非按内容尺寸的 AUTO。
    let mut cs = ComputedStyle::new();
    cs.set("flex-basis", num(0.0));
    let style = map_style(Some(&cs));
    assert_eq!(style.flex_basis, Dimension::length(0.0));
}

#[test]
fn width_calc_percentage_resolves() {
    // P1-9: calc(50%) 之前静默降级 AUTO，应递归展开取内层百分比。
    let mut cs = ComputedStyle::new();
    cs.set("width", calc_pct(50.0));
    let style = map_style(Some(&cs));
    assert_eq!(style.size.width, Dimension::percent(0.5));
}

#[test]
fn width_calc_px_resolves() {
    // P1-9: calc(20px) 递归展开取内层 px。
    let mut cs = ComputedStyle::new();
    cs.set("width", calc_px(20.0));
    let style = map_style(Some(&cs));
    assert_eq!(style.size.width, Dimension::length(20.0));
}

#[test]
fn padding_calc_px_resolves() {
    // P1-9: padding: calc(...) 修复前 → ZERO。
    let mut cs = ComputedStyle::new();
    cs.set("padding-top", calc_px(12.0));
    let style = map_style(Some(&cs));
    assert_eq!(style.padding.top, LengthPercentage::length(12.0));
}

// —— margin ——

#[test]
fn margin_px_maps_correctly() {
    let mut cs = ComputedStyle::new();
    cs.set("margin-top", px(10.0));
    cs.set("margin-right", px(20.0));
    cs.set("margin-bottom", px(30.0));
    cs.set("margin-left", px(40.0));
    let style = map_style(Some(&cs));
    assert_eq!(style.margin.top, LengthPercentageAuto::length(10.0));
    assert_eq!(style.margin.right, LengthPercentageAuto::length(20.0));
    assert_eq!(style.margin.bottom, LengthPercentageAuto::length(30.0));
    assert_eq!(style.margin.left, LengthPercentageAuto::length(40.0));
}

#[test]
fn margin_auto_maps_correctly() {
    let mut cs = ComputedStyle::new();
    cs.set("margin-left", kw("auto"));
    let style = map_style(Some(&cs));
    assert!(
        style.margin.left.is_auto(),
        "margin-left: auto 应映射为 AUTO"
    );
}

#[test]
fn margin_percentage_maps_correctly() {
    let mut cs = ComputedStyle::new();
    cs.set("margin-top", pct(10.0));
    let style = map_style(Some(&cs));
    assert_eq!(style.margin.top, LengthPercentageAuto::percent(0.1));
}

// —— padding ——

#[test]
fn padding_px_maps_correctly() {
    let mut cs = ComputedStyle::new();
    cs.set("padding-top", px(5.0));
    cs.set("padding-left", px(15.0));
    let style = map_style(Some(&cs));
    assert_eq!(style.padding.top, LengthPercentage::length(5.0));
    assert_eq!(style.padding.left, LengthPercentage::length(15.0));
}

#[test]
fn padding_defaults_to_zero() {
    let cs = ComputedStyle::new();
    let style = map_style(Some(&cs));
    assert_eq!(style.padding.top, LengthPercentage::ZERO);
}

// —— box-sizing ——

#[test]
fn box_sizing_content_box() {
    let mut cs = ComputedStyle::new();
    cs.set("box-sizing", kw("content-box"));
    let style = map_style(Some(&cs));
    assert_eq!(style.box_sizing, BoxSizing::ContentBox);
}

#[test]
fn box_sizing_border_box() {
    let mut cs = ComputedStyle::new();
    cs.set("box-sizing", kw("border-box"));
    let style = map_style(Some(&cs));
    assert_eq!(style.box_sizing, BoxSizing::BorderBox);
}

#[test]
fn box_sizing_defaults_to_content_box() {
    // CSS Box Model §4.1: box-sizing 初始值为 content-box.
    // taffy Style::default() 使用 BorderBox，我们的 map_style 必须纠正为 ContentBox.
    let cs = ComputedStyle::new();
    let style = map_style(Some(&cs));
    assert_eq!(
        style.box_sizing,
        BoxSizing::ContentBox,
        "box-sizing 未设置时应为初始值 ContentBox，而非 taffy 默认的 BorderBox"
    );
}

#[test]
fn box_sizing_unknown_value_falls_back_to_content_box() {
    // CSS Cascade §7.1: 无效值回退到初始值.
    // box-sizing 初始值为 content-box.
    let mut cs = ComputedStyle::new();
    cs.set("box-sizing", kw("invalid-value"));
    let style = map_style(Some(&cs));
    assert_eq!(
        style.box_sizing,
        BoxSizing::ContentBox,
        "未知 box-sizing 值应回退到初始值 ContentBox"
    );
}

// —— flexbox 属性 ——

#[test]
fn flex_direction_row() {
    let mut cs = ComputedStyle::new();
    cs.set("flex-direction", kw("row"));
    let style = map_style(Some(&cs));
    assert_eq!(style.flex_direction, FlexDirection::Row);
}

#[test]
fn flex_direction_column() {
    let mut cs = ComputedStyle::new();
    cs.set("flex-direction", kw("column"));
    let style = map_style(Some(&cs));
    assert_eq!(style.flex_direction, FlexDirection::Column);
}

#[test]
fn flex_direction_row_reverse() {
    let mut cs = ComputedStyle::new();
    cs.set("flex-direction", kw("row-reverse"));
    let style = map_style(Some(&cs));
    assert_eq!(style.flex_direction, FlexDirection::RowReverse);
}

#[test]
fn flex_wrap_wrap() {
    let mut cs = ComputedStyle::new();
    cs.set("flex-wrap", kw("wrap"));
    let style = map_style(Some(&cs));
    assert_eq!(style.flex_wrap, FlexWrap::Wrap);
}

#[test]
fn flex_wrap_nowrap() {
    let mut cs = ComputedStyle::new();
    cs.set("flex-wrap", kw("nowrap"));
    let style = map_style(Some(&cs));
    assert_eq!(style.flex_wrap, FlexWrap::NoWrap);
}

#[test]
fn justify_content_center() {
    let mut cs = ComputedStyle::new();
    cs.set("justify-content", kw("center"));
    let style = map_style(Some(&cs));
    assert_eq!(style.justify_content, Some(JustifyContent::CENTER));
}

#[test]
fn justify_content_space_between() {
    let mut cs = ComputedStyle::new();
    cs.set("justify-content", kw("space-between"));
    let style = map_style(Some(&cs));
    assert_eq!(style.justify_content, Some(JustifyContent::SPACE_BETWEEN));
}

#[test]
fn justify_content_end_maps_to_flex_end() {
    // P2-10: justify-content: end → FLEX_END（修复前落入默认 flex-start）。
    let mut cs = ComputedStyle::new();
    cs.set("justify-content", kw("end"));
    let style = map_style(Some(&cs));
    assert_eq!(style.justify_content, Some(JustifyContent::FLEX_END));
}

#[test]
fn justify_content_right_maps_to_flex_end() {
    // P2-10: justify-content: right → FLEX_END。
    let mut cs = ComputedStyle::new();
    cs.set("justify-content", kw("right"));
    let style = map_style(Some(&cs));
    assert_eq!(style.justify_content, Some(JustifyContent::FLEX_END));
}

#[test]
fn justify_content_start_maps_to_flex_start() {
    // P2-10: justify-content: start → FLEX_START（显式映射）。
    let mut cs = ComputedStyle::new();
    cs.set("justify-content", kw("start"));
    let style = map_style(Some(&cs));
    assert_eq!(style.justify_content, Some(JustifyContent::FLEX_START));
}

#[test]
fn justify_content_normal_falls_back_to_default() {
    // P2-10: justify-content: normal → None（flex 布局默认 flex-start）。
    let mut cs = ComputedStyle::new();
    cs.set("justify-content", kw("normal"));
    let style = map_style(Some(&cs));
    assert_eq!(style.justify_content, None);
}

#[test]
fn align_items_center() {
    let mut cs = ComputedStyle::new();
    cs.set("align-items", kw("center"));
    let style = map_style(Some(&cs));
    assert_eq!(style.align_items, Some(AlignItems::CENTER));
}

#[test]
fn align_items_stretch() {
    let mut cs = ComputedStyle::new();
    cs.set("align-items", kw("stretch"));
    let style = map_style(Some(&cs));
    assert_eq!(style.align_items, Some(AlignItems::STRETCH));
}

#[test]
fn align_items_normal_falls_back_to_stretch() {
    // P0-2: CSS Box Alignment §7.1: align-items: normal 在 flex 布局中行为
    // 等价于 stretch（交叉轴拉伸填满容器）。映射为 None 让 taffy 用默认
    // STRETCH，而非之前错误映射的 flex-start（使所有 flex 容器默认失去拉伸）。
    let mut cs = ComputedStyle::new();
    cs.set("align-items", kw("normal"));
    let style = map_style(Some(&cs));
    assert_eq!(
        style.align_items, None,
        "align-items: normal 应回退到 taffy 默认 STRETCH（None），而非 flex-start"
    );
}

#[test]
fn align_self_auto_falls_back_to_none() {
    let mut cs = ComputedStyle::new();
    cs.set("align-self", kw("auto"));
    let style = map_style(Some(&cs));
    assert_eq!(
        style.align_self, None,
        "align-self: auto 应回退到 None（继承父 align-items）"
    );
}

#[test]
fn align_self_center() {
    let mut cs = ComputedStyle::new();
    cs.set("align-self", kw("center"));
    let style = map_style(Some(&cs));
    assert_eq!(style.align_self, Some(AlignSelf::CENTER));
}

// —— align-content ——

#[test]
fn align_content_space_between_maps() {
    // P2-11: align-content 之前完全未映射。
    let mut cs = ComputedStyle::new();
    cs.set("align-content", kw("space-between"));
    let style = map_style(Some(&cs));
    assert_eq!(style.align_content, Some(AlignContent::SPACE_BETWEEN));
}

#[test]
fn align_content_stretch_maps() {
    let mut cs = ComputedStyle::new();
    cs.set("align-content", kw("stretch"));
    let style = map_style(Some(&cs));
    assert_eq!(style.align_content, Some(AlignContent::STRETCH));
}

#[test]
fn align_content_normal_falls_back_to_none() {
    // P2-11: normal → None（stretch 语义，交 taffy 默认）。
    let mut cs = ComputedStyle::new();
    cs.set("align-content", kw("normal"));
    let style = map_style(Some(&cs));
    assert_eq!(style.align_content, None);
}

// —— flex-grow / flex-shrink / flex-basis ——

#[test]
fn flex_grow_number() {
    let mut cs = ComputedStyle::new();
    cs.set("flex-grow", num(2.0));
    let style = map_style(Some(&cs));
    assert_eq!(style.flex_grow, 2.0);
}

#[test]
fn flex_shrink_number() {
    let mut cs = ComputedStyle::new();
    cs.set("flex-shrink", num(0.5));
    let style = map_style(Some(&cs));
    assert_eq!(style.flex_shrink, 0.5);
}

#[test]
fn flex_grow_negative_value_rejected() {
    // CSS Flexbox §7.2: "Negative values are invalid."
    // 负的 flex-grow 应被拒绝，保持初始值 0.0.
    let mut cs = ComputedStyle::new();
    cs.set("flex-grow", num(-1.0));
    let style = map_style(Some(&cs));
    assert_eq!(
        style.flex_grow, 0.0,
        "负的 flex-grow 应被拒绝，保持初始值 0.0"
    );
}

#[test]
fn flex_shrink_negative_value_rejected() {
    // CSS Flexbox §7.2: "Negative values are invalid."
    // 负的 flex-shrink 应被拒绝，保持初始值 1.0.
    let mut cs = ComputedStyle::new();
    cs.set("flex-shrink", num(-0.5));
    let style = map_style(Some(&cs));
    assert_eq!(
        style.flex_shrink, 1.0,
        "负的 flex-shrink 应被拒绝，保持 taffy 默认值 1.0"
    );
}

#[test]
fn flex_basis_px() {
    let mut cs = ComputedStyle::new();
    cs.set("flex-basis", px(150.0));
    let style = map_style(Some(&cs));
    assert_eq!(style.flex_basis, Dimension::length(150.0));
}

#[test]
fn flex_basis_auto() {
    let mut cs = ComputedStyle::new();
    cs.set("flex-basis", kw("auto"));
    let style = map_style(Some(&cs));
    assert!(
        style.flex_basis.is_auto(),
        "flex-basis: auto 应映射为 Dimension::AUTO"
    );
}

// —— gap（仅 row-gap / column-gap 长属性）——
//
// `gap` 简写在 cascade 收集时已展开为 `row-gap` + `column-gap`（P2-9/B8），
// 布局层只读长属性，不再直接读 `gap`。

#[test]
fn row_gap_and_column_gap_independent() {
    let mut cs = ComputedStyle::new();
    cs.set("row-gap", px(5.0));
    cs.set("column-gap", px(15.0));
    let style = map_style(Some(&cs));
    assert_eq!(style.gap.height, LengthPercentage::length(5.0));
    assert_eq!(style.gap.width, LengthPercentage::length(15.0));
}

#[test]
fn row_gap_and_column_gap_percentages_map() {
    // P2-8: 百分比 gap 不丢失（taffy 原生支持）。gap 简写含百分比时在
    // cascade 展开为 row-gap/column-gap 百分比，映射层经 map_length_percentage
    // 保留为 percent。
    let mut cs = ComputedStyle::new();
    cs.set("row-gap", pct(5.0));
    cs.set("column-gap", pct(20.0));
    let style = map_style(Some(&cs));
    assert_eq!(style.gap.height, LengthPercentage::percent(0.05));
    assert_eq!(style.gap.width, LengthPercentage::percent(0.20));
}
