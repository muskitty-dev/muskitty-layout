//! ComputedStyle → taffy::Style 映射。
//!
//! [`map_style`] 将 [`ComputedStyle`] 中的 display / 尺寸 / margin / padding /
//! flexbox 等属性映射为 taffy 的 [`Style`] 结构体，作为布局引擎的输入。
//!
//! # 规范依据
//!
//! - CSS Box Model Level 3 §2-§3: margin/border/padding/content、box-sizing
//! - CSS Flexbox Level 1 §4-§8: flex-direction/flex-wrap/justify/align/gap
//! - CSS Display Level 3 §2: display

use muskitty_cascade::{ComputedStyle, ComputedValue};
use muskitty_css::parser::ComponentValue;
use muskitty_css::tokenizer::Token;
use taffy::geometry::Rect;
use taffy::style::{
    AlignContent, AlignItems, BoxSizing, Dimension, Display, FlexDirection, FlexWrap,
    GridTemplateComponent, JustifyContent, LengthPercentage, LengthPercentageAuto, Position, Style,
};
use taffy::style_helpers::{fr, length, percent, TaffyAuto, TaffyZero};

/// 将 [`ComputedStyle`] 映射为 taffy [`Style`]。
///
/// `computed` 为 `None` 时返回默认 Style（display 默认为 `Block`）。
/// 已识别的属性会被覆盖到 Style 对应字段；未识别的属性保持 taffy 默认值。
pub(crate) fn map_style(computed: Option<&ComputedStyle>) -> Style {
    // taffy 默认 display 为 Flex（启用 flexbox feature 时），但 HTML 块级元素
    // 的预期默认是 Block，因此显式覆盖为 Block。
    // 同时 taffy 默认 box_sizing 为 BorderBox，与 CSS Box Model §4.1 规定的初始值
    // content-box 不一致，这里纠正为 ContentBox。
    let mut style = Style {
        display: Display::Block,
        box_sizing: BoxSizing::ContentBox,
        ..Style::default()
    };

    let cs = match computed {
        Some(cs) => cs,
        None => return style,
    };

    // —— display ——
    // CSS Display Level 3 §2: display 属性映射。
    //
    // taffy 0.12 的 Display 枚举只有 Block/Flex/Grid/None，无 Inline 变体。
    // 处理策略：
    // - `block` / `flow` / `flow-root` → Block（block container）
    // - `flex` / `inline-flex` → Flex（inline-flex 的 inline-level 行为 taffy 不支持，
    //   仅保留 flex container 语义）
    // - `grid` / `inline-grid` → Grid（同上）
    // - `inline` / `inline-block` → Block（taffy 无 inline layout，作为 workaround）
    // - `none` → None（不生成 box，已在 build_layout_tree 中排除）
    // - `contents` → Block（TODO: §2.5 规定元素本身不生成 box 但保留子元素，
    //   需在 build_layout_tree 中特殊处理，当前先当 Block）
    // - `list-item` → Block（TODO: 需额外处理 marker box，当前当 Block）
    if let Some(kw) = get_keyword(cs, "display") {
        style.display = if kw_eq(kw, "flex") || kw_eq(kw, "inline-flex") {
            Display::Flex
        } else if kw_eq(kw, "grid") || kw_eq(kw, "inline-grid") {
            Display::Grid
        } else if kw_eq(kw, "none") {
            Display::None
        } else {
            // block / flow / flow-root / inline / inline-block / contents / list-item
            // 及未知关键字 → Block（见函数头注释）
            Display::Block
        };
    }

    // —— position ——
    // CSS Positioned Layout Level 3 §2: position 属性。
    //
    // taffy 0.12 仅 Relative/Absolute（无 Fixed）。`fixed` 近似映射为
    // Absolute：taffy 的 absolute 元素相对 closest positioned ancestor
    // （无则相对 origin），在无 positioned ancestor 时等价于 viewport 定位。
    if let Some(kw) = get_keyword(cs, "position") {
        style.position = if kw_eq(kw, "absolute") || kw_eq(kw, "fixed") {
            Position::Absolute
        } else {
            // static / relative / sticky / 未知 → Relative（static 默认无偏移）
            Position::Relative
        };
    }

    // —— top / right / bottom / left（inset）——
    // CSS Positioned Layout Level 3 §4: 偏移属性。auto = 不偏移。
    style.inset = Rect {
        top: map_length_percentage_auto(cs.get("top")),
        right: map_length_percentage_auto(cs.get("right")),
        bottom: map_length_percentage_auto(cs.get("bottom")),
        left: map_length_percentage_auto(cs.get("left")),
    };

    // —— width / height ——
    if let Some(cv) = cs.get("width") {
        style.size.width = map_dimension(cv);
    }
    if let Some(cv) = cs.get("height") {
        style.size.height = map_dimension(cv);
    }

    // —— min-width / max-width / min-height / max-height ——
    if let Some(cv) = cs.get("min-width") {
        style.min_size.width = map_dimension(cv);
    }
    if let Some(cv) = cs.get("max-width") {
        style.max_size.width = map_dimension(cv);
    }
    if let Some(cv) = cs.get("min-height") {
        style.min_size.height = map_dimension(cv);
    }
    if let Some(cv) = cs.get("max-height") {
        style.max_size.height = map_dimension(cv);
    }

    // —— margin ——
    style.margin = Rect {
        top: map_length_percentage_auto(cs.get("margin-top")),
        right: map_length_percentage_auto(cs.get("margin-right")),
        bottom: map_length_percentage_auto(cs.get("margin-bottom")),
        left: map_length_percentage_auto(cs.get("margin-left")),
    };

    // —— padding ——
    style.padding = Rect {
        top: map_length_percentage(cs.get("padding-top")),
        right: map_length_percentage(cs.get("padding-right")),
        bottom: map_length_percentage(cs.get("padding-bottom")),
        left: map_length_percentage(cs.get("padding-left")),
    };

    // —— box-sizing ——
    // CSS Box Model Level 3 §4.1: 初始值为 content-box.
    // 未知值按 §7.1 回退到初始值（ContentBox），而非 taffy 默认的 BorderBox.
    if let Some(kw) = get_keyword(cs, "box-sizing") {
        style.box_sizing = if kw_eq(kw, "border-box") {
            BoxSizing::BorderBox
        } else {
            // content-box 及未知值 → ContentBox（§7.1 回退初始值）
            BoxSizing::ContentBox
        };
    }

    // —— flex-direction ——
    if let Some(kw) = get_keyword(cs, "flex-direction") {
        style.flex_direction = if kw_eq(kw, "row-reverse") {
            FlexDirection::RowReverse
        } else if kw_eq(kw, "column") {
            FlexDirection::Column
        } else if kw_eq(kw, "column-reverse") {
            FlexDirection::ColumnReverse
        } else {
            // row 及未知值 → Row
            FlexDirection::Row
        };
    }

    // —— flex-wrap ——
    if let Some(kw) = get_keyword(cs, "flex-wrap") {
        style.flex_wrap = if kw_eq(kw, "wrap") {
            FlexWrap::Wrap
        } else if kw_eq(kw, "wrap-reverse") {
            FlexWrap::WrapReverse
        } else {
            // nowrap 及未知值 → NoWrap
            FlexWrap::NoWrap
        };
    }

    // —— justify-content ——
    if let Some(kw) = get_keyword(cs, "justify-content") {
        style.justify_content = map_justify_content(kw);
    }

    // —— align-content ——
    // CSS Box Alignment Level 3 §8.1: 多行 flex 容器的交叉轴行分布。
    if let Some(kw) = get_keyword(cs, "align-content") {
        style.align_content = map_align_content(kw);
    }

    // —— align-items ——
    if let Some(kw) = get_keyword(cs, "align-items") {
        style.align_items = map_align_items(kw);
    }

    // —— align-self ——
    if let Some(kw) = get_keyword(cs, "align-self") {
        // align-self: auto 表示继承父元素的 align-items，用 None 表示回退。
        if !kw_eq(kw, "auto") {
            style.align_self = map_align_items(kw);
        }
    }

    // —— flex-grow / flex-shrink ——
    // CSS Flexbox §7.2: "Negative values are invalid."
    // 负值被拒绝：
    // - flex-grow 保持初始值 0.0（CSS）= taffy 默认值 0.0
    // - flex-shrink 保持初始值 1.0（CSS）= taffy 默认值 1.0
    if let Some(cv) = cs.get("flex-grow") {
        if let Some(val) = extract_number_from_cv(cv) {
            if val >= 0.0 {
                style.flex_grow = val;
            }
        }
    }
    if let Some(cv) = cs.get("flex-shrink") {
        if let Some(val) = extract_number_from_cv(cv) {
            if val >= 0.0 {
                style.flex_shrink = val;
            }
        }
    }

    // —— flex-basis ——
    if let Some(cv) = cs.get("flex-basis") {
        style.flex_basis = map_dimension(cv);
    }

    // —— row-gap / column-gap ——
    // CSS Box Alignment Level 3 §6.2: `gap` 简写已在 cascade 收集时展开为
    // `row-gap` + `column-gap`（P2-9/B8），此处只读长属性。百分比 gap 经
    // [`map_length_percentage`] 保留（taffy 原生支持百分比 gap，P2-8）。
    if let Some(cv) = cs.get("column-gap") {
        style.gap.width = map_length_percentage(Some(cv));
    }
    if let Some(cv) = cs.get("row-gap") {
        style.gap.height = map_length_percentage(Some(cv));
    }

    // —— grid-template-columns / rows（CSS Grid Layout Level 1 §7）——
    if let Some(cv) = cs.get("grid-template-columns") {
        style.grid_template_columns = map_grid_template(cv);
    }
    if let Some(cv) = cs.get("grid-template-rows") {
        style.grid_template_rows = map_grid_template(cv);
    }

    style
}

// —— 辅助函数 ——

/// 从 ComputedStyle 中读取指定属性的关键字值。
///
/// 单态化（P2-20）后关键字即首个 Ident token，直接退化到
/// [`ComputedValue::keyword`]（defaulting 初始值与 cascade pipeline
/// 产物统一处理）。返回借用（零分配，PERF-10），调用方用
/// `eq_ignore_ascii_case` 比较（`kw_eq`）。
fn get_keyword<'a>(cs: &'a ComputedStyle, name: &str) -> Option<&'a str> {
    cs.get(name).and_then(|cv| cv.keyword())
}

/// 关键字大小写不敏感比较（CSS 关键字均为 ASCII）。
///
/// PERF-10：替代 `to_ascii_lowercase()` 的临时分配，与 [`get_keyword`]
/// 的借用返回配合实现 map_style 每属性零分配。
fn kw_eq(s: &str, expected: &str) -> bool {
    s.eq_ignore_ascii_case(expected)
}

/// `justify-content` 关键字 → [`JustifyContent`]。
///
/// # 规范依据
///
/// CSS Box Alignment Level 3 §5.1: `start`/`end` 是逻辑对齐关键字（writing-mode
/// 相对），`left`/`right` 是物理对齐关键字；在 LTR flex 布局中均等价于对应
/// flex 边。`normal` 行为等价于 `start`（flex 布局默认 flex-start），返回 `None`
/// 让 taffy 默认值生效（P2-10）。
fn map_justify_content(kw: &str) -> Option<JustifyContent> {
    Some(match kw {
        _ if kw_eq(kw, "flex-start") || kw_eq(kw, "start") || kw_eq(kw, "left") => {
            JustifyContent::FLEX_START
        }
        _ if kw_eq(kw, "center") => JustifyContent::CENTER,
        _ if kw_eq(kw, "flex-end") || kw_eq(kw, "end") || kw_eq(kw, "right") => {
            JustifyContent::FLEX_END
        }
        _ if kw_eq(kw, "space-between") => JustifyContent::SPACE_BETWEEN,
        _ if kw_eq(kw, "space-around") => JustifyContent::SPACE_AROUND,
        _ if kw_eq(kw, "space-evenly") => JustifyContent::SPACE_EVENLY,
        _ => return None,
    })
}

/// `align-content` 关键字 → [`AlignContent`]。
///
/// # 规范依据
///
/// CSS Box Alignment Level 3 §8.1: 多行 flex 容器的交叉轴行分布。
/// `normal` 行为等价于 `stretch`（默认行拉伸），返回 `None` 让 taffy 默认值
/// 生效（P2-11）。
fn map_align_content(kw: &str) -> Option<AlignContent> {
    Some(match kw {
        _ if kw_eq(kw, "stretch") => AlignContent::STRETCH,
        _ if kw_eq(kw, "flex-start") || kw_eq(kw, "start") => AlignContent::FLEX_START,
        _ if kw_eq(kw, "center") => AlignContent::CENTER,
        _ if kw_eq(kw, "flex-end") || kw_eq(kw, "end") => AlignContent::FLEX_END,
        _ if kw_eq(kw, "space-between") => AlignContent::SPACE_BETWEEN,
        _ if kw_eq(kw, "space-around") => AlignContent::SPACE_AROUND,
        _ if kw_eq(kw, "space-evenly") => AlignContent::SPACE_EVENLY,
        _ => return None,
    })
}

/// `align-items` / `align-self` 关键字 → [`AlignItems`]。
///
/// # 规范依据
///
/// - CSS Box Alignment Level 3 §7.1: `align-items: normal` 在 flex 布局中
///   行为等价于 `stretch`（交叉轴拉伸填满容器），因此返回 `None` 让 taffy
///   用默认值 `STRETCH`（P0-2）。之前错误映射为 `flex-start` 使所有 flex
///   容器默认失去拉伸。
/// - `start` / `end` 是逻辑对齐关键字（§5.1），映射为对应的 flex 边。
/// - CSS Flexbox §8.3: `start` 对齐到 cross-axis 起始端，等价于 `flex-start`。
fn map_align_items(kw: &str) -> Option<AlignItems> {
    Some(match kw {
        _ if kw_eq(kw, "stretch") => AlignItems::STRETCH,
        _ if kw_eq(kw, "flex-start") || kw_eq(kw, "start") => AlignItems::FLEX_START,
        _ if kw_eq(kw, "center") => AlignItems::CENTER,
        _ if kw_eq(kw, "flex-end") || kw_eq(kw, "end") => AlignItems::FLEX_END,
        _ if kw_eq(kw, "baseline") => AlignItems::BASELINE,
        // normal（→ stretch）与未知值统一返回 None，交给 taffy 默认值。
        _ => return None,
    })
}

/// 将 [`ComputedValue`] 映射为 [`Dimension`]（用于 width/height/min-max/flex-basis）。
///
/// - `auto` → [`Dimension::AUTO`]
/// - `px` 长度 → [`Dimension::length`]
/// - 百分比 → [`Dimension::percent`]（0.0-1.0 区间）
fn map_dimension(cv: &ComputedValue) -> Dimension {
    // 单态化（P2-20）：关键字即首个 Ident，其余按 token 序列解析。
    if let Some(kw) = cv.keyword() {
        if kw.eq_ignore_ascii_case("auto") || kw.eq_ignore_ascii_case("none") {
            return Dimension::AUTO;
        }
    }
    let cvs = cv.tokens();
    if let Some(px) = extract_px(cvs) {
        Dimension::length(px)
    } else if let Some(pct) = extract_percent(cvs) {
        Dimension::percent(pct / 100.0)
    } else {
        Dimension::AUTO
    }
}

/// 将 `Option<&ComputedValue>` 映射为 [`LengthPercentageAuto`]（用于 margin）。
///
/// CSS margin 的初始值为 `0`（非 `auto`），因此属性未设置（`None`）或
/// 值无法解析时返回 [`LengthPercentageAuto::ZERO`]。仅 `auto` 关键字
/// 映射为 [`LengthPercentageAuto::AUTO`]。
fn map_length_percentage_auto(cv: Option<&ComputedValue>) -> LengthPercentageAuto {
    match cv {
        Some(cv) => {
            if let Some(kw) = cv.keyword() {
                if kw.eq_ignore_ascii_case("auto") {
                    return LengthPercentageAuto::AUTO;
                }
            }
            let cvs = cv.tokens();
            if let Some(px) = extract_px(cvs) {
                LengthPercentageAuto::length(px)
            } else if let Some(pct) = extract_percent(cvs) {
                LengthPercentageAuto::percent(pct / 100.0)
            } else {
                LengthPercentageAuto::ZERO
            }
        }
        None => LengthPercentageAuto::ZERO,
    }
}

/// 将 `Option<&ComputedValue>` 映射为 [`LengthPercentage`]（用于 padding/gap）。
fn map_length_percentage(cv: Option<&ComputedValue>) -> LengthPercentage {
    match cv {
        Some(cv) => {
            let cvs = cv.tokens();
            if let Some(px) = extract_px(cvs) {
                LengthPercentage::length(px)
            } else if let Some(pct) = extract_percent(cvs) {
                LengthPercentage::percent(pct / 100.0)
            } else {
                LengthPercentage::ZERO
            }
        }
        None => LengthPercentage::ZERO,
    }
}

/// 从 component value 列表中提取第一个 `px` 长度值。
///
/// - P1-8: 顶层裸 `0`（`Token::Number(0)`）是合法 `<length>`（CSS Values
///   Level 4 §5.1），映射为 `length(0.0)` 而非回退 AUTO。`margin: 0` 等恰好
///   因 fallback 到 ZERO 碰巧正确，`width: 0` / `min-width: 0` / `flex-basis: 0`
///   却错误落到 AUTO。
/// - P1-9: `calc(...)` 在 cascade 中保留为 `Function`，递归展开取内层首个
///   有效 px（短期方案；长期由 cascade 层做 calc 求值）。
fn extract_px(cvs: &[ComponentValue]) -> Option<f32> {
    for cv in cvs {
        match cv {
            ComponentValue::PreservedToken(Token::Dimension(numeric, unit))
                if unit.eq_ignore_ascii_case("px") =>
            {
                return Some(numeric.value as f32);
            }
            ComponentValue::PreservedToken(Token::Number(n)) if n.value == 0.0 => {
                return Some(0.0);
            }
            ComponentValue::Function(f) => {
                if let Some(px) = extract_px(&f.value) {
                    return Some(px);
                }
            }
            _ => {}
        }
    }
    None
}

/// 从 component value 列表中提取第一个百分比值（0.0-100.0）。
///
/// P1-9: 同 [`extract_px`]，递归展开 `calc(...)` Function 取内层百分比。
fn extract_percent(cvs: &[ComponentValue]) -> Option<f32> {
    for cv in cvs {
        match cv {
            ComponentValue::PreservedToken(Token::Percentage(numeric)) => {
                return Some(numeric.value as f32);
            }
            ComponentValue::Function(f) => {
                if let Some(pct) = extract_percent(&f.value) {
                    return Some(pct);
                }
            }
            _ => {}
        }
    }
    None
}

/// 从 [`ComputedValue`] 中提取第一个数字值（用于 flex-grow/flex-shrink）。
fn extract_number_from_cv(cv: &ComputedValue) -> Option<f32> {
    for cv in cv.tokens() {
        if let ComponentValue::PreservedToken(Token::Number(numeric)) = cv {
            return Some(numeric.value as f32);
        }
    }
    None
}

/// 将 `grid-template-columns/rows` 的 component value 列表解析为 taffy track 列表。
///
/// 支持 `fr` / `px` / `%` / `auto` 单 track（`repeat()` 与命名线推迟，L-3）。
fn map_grid_template(cv: &ComputedValue) -> Vec<GridTemplateComponent<String>> {
    let mut tracks = Vec::new();
    for token in cv.tokens() {
        match token {
            ComponentValue::PreservedToken(Token::Dimension(numeric, unit)) => {
                if unit.eq_ignore_ascii_case("fr") {
                    tracks.push(fr(numeric.value as f32));
                } else if unit.eq_ignore_ascii_case("px") {
                    tracks.push(length(numeric.value as f32));
                }
            }
            ComponentValue::PreservedToken(Token::Percentage(numeric)) => {
                tracks.push(percent(numeric.value as f32));
            }
            ComponentValue::PreservedToken(Token::Ident(s)) if s.eq_ignore_ascii_case("auto") => {
                tracks.push(GridTemplateComponent::AUTO);
            }
            _ => {}
        }
    }
    tracks
}

#[cfg(test)]
mod tests {
    use super::*;
    use muskitty_cascade::{ComputedStyle, ComputedValue};
    use muskitty_css::parser::ComponentValue;
    use muskitty_css::tokenizer::{Numeric, Token};
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
}
