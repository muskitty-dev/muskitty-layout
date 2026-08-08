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
    JustifyContent, LengthPercentage, LengthPercentageAuto, Style,
};
use taffy::style_helpers::{TaffyAuto, TaffyZero};

/// 将 [`ComputedStyle`] 映射为 taffy [`Style`]。
///
/// `computed` 为 `None` 时返回默认 Style（display 默认为 `Block`）。
/// 已识别的属性会被覆盖到 Style 对应字段；未识别的属性保持 taffy 默认值。
pub fn map_style(computed: Option<&ComputedStyle>) -> Style {
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
        style.display = match kw.to_ascii_lowercase().as_str() {
            "flex" | "inline-flex" => Display::Flex,
            "grid" | "inline-grid" => Display::Grid,
            "block" | "flow" | "flow-root" | "inline" | "inline-block" | "contents"
            | "list-item" => Display::Block,
            "none" => Display::None,
            _ => Display::Block,
        };
    }

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
        style.box_sizing = match kw.to_ascii_lowercase().as_str() {
            "content-box" => BoxSizing::ContentBox,
            "border-box" => BoxSizing::BorderBox,
            _ => BoxSizing::ContentBox,
        };
    }

    // —— flex-direction ——
    if let Some(kw) = get_keyword(cs, "flex-direction") {
        style.flex_direction = match kw.to_ascii_lowercase().as_str() {
            "row" => FlexDirection::Row,
            "row-reverse" => FlexDirection::RowReverse,
            "column" => FlexDirection::Column,
            "column-reverse" => FlexDirection::ColumnReverse,
            _ => FlexDirection::Row,
        };
    }

    // —— flex-wrap ——
    if let Some(kw) = get_keyword(cs, "flex-wrap") {
        style.flex_wrap = match kw.to_ascii_lowercase().as_str() {
            "nowrap" => FlexWrap::NoWrap,
            "wrap" => FlexWrap::Wrap,
            "wrap-reverse" => FlexWrap::WrapReverse,
            _ => FlexWrap::NoWrap,
        };
    }

    // —— justify-content ——
    if let Some(kw) = get_keyword(cs, "justify-content") {
        style.justify_content = map_justify_content(&kw.to_ascii_lowercase());
    }

    // —— align-content ——
    // CSS Box Alignment Level 3 §8.1: 多行 flex 容器的交叉轴行分布。
    if let Some(kw) = get_keyword(cs, "align-content") {
        style.align_content = map_align_content(&kw.to_ascii_lowercase());
    }

    // —— align-items ——
    if let Some(kw) = get_keyword(cs, "align-items") {
        style.align_items = map_align_items(&kw.to_ascii_lowercase());
    }

    // —— align-self ——
    if let Some(kw) = get_keyword(cs, "align-self") {
        // align-self: auto 表示继承父元素的 align-items，用 None 表示回退。
        if !kw.eq_ignore_ascii_case("auto") {
            style.align_self = map_align_items(&kw.to_ascii_lowercase());
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

    style
}

// —— 辅助函数 ——

/// 从 ComputedStyle 中读取指定属性的关键字值（小写化由调用方决定）。
///
/// 单态化（P2-20）后关键字即首个 Ident token，直接退化到
/// [`ComputedValue::keyword`]（defaulting 初始值与 cascade pipeline
/// 产物统一处理）。
fn get_keyword(cs: &ComputedStyle, name: &str) -> Option<String> {
    cs.get(name)
        .and_then(|cv| cv.keyword().map(|s| s.to_string()))
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
        "flex-start" | "start" | "left" => JustifyContent::FLEX_START,
        "center" => JustifyContent::CENTER,
        "flex-end" | "end" | "right" => JustifyContent::FLEX_END,
        "space-between" => JustifyContent::SPACE_BETWEEN,
        "space-around" => JustifyContent::SPACE_AROUND,
        "space-evenly" => JustifyContent::SPACE_EVENLY,
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
        "stretch" => AlignContent::STRETCH,
        "flex-start" | "start" => AlignContent::FLEX_START,
        "center" => AlignContent::CENTER,
        "flex-end" | "end" => AlignContent::FLEX_END,
        "space-between" => AlignContent::SPACE_BETWEEN,
        "space-around" => AlignContent::SPACE_AROUND,
        "space-evenly" => AlignContent::SPACE_EVENLY,
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
        "stretch" => AlignItems::STRETCH,
        "flex-start" | "start" => AlignItems::FLEX_START,
        "center" => AlignItems::CENTER,
        "flex-end" | "end" => AlignItems::FLEX_END,
        "baseline" => AlignItems::BASELINE,
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
