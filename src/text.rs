//! 文本测量（layout 层）。
//!
//! 用 cosmic-text 测量 text 节点的自然尺寸，供 [`crate::convert::build_layout_tree`]
//! 将 text 节点作为固定尺寸的 taffy leaf 加入布局树（T-1 单行 / T-3 字体属性）。
//!
//! # 规范依据
//!
//! - CSS Fonts Level 3: font-size / font-family / font-weight / line-height
//! - CSS Inline Layout Level 3: inline 盒尺寸（inline 流合并推迟，当前单行）

use cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Weight};
use muskitty_cascade::ComputedStyle;
use muskitty_css::parser::ComponentValue;
use muskitty_css::tokenizer::Token;

/// 浏览器默认 font-size（px）。CSS `medium` = 16px。
pub(crate) const DEFAULT_FONT_SIZE: f32 = 16.0;

/// 默认字体族（CSS `font-family` 初始值 `serif`）。
pub(crate) const DEFAULT_FONT_FAMILY: &str = "serif";

/// 默认字重（CSS `font-weight` 初始值 `normal` = 400）。
pub(crate) const DEFAULT_FONT_WEIGHT: u16 = 400;

/// 测量文本的自然尺寸。
///
/// `max_width` 为 `Some(w)` 时按宽度 `w` 换行（T-3），`None` 时单行不换行。
/// `line_height` 为 `line-height` 的使用值（px，M-3 batch 3）：由 cascade
/// `text_props::used_line_height_px` 解析（含 `normal`/数/长度语义与继承倍数
/// 折算），直接作为 cosmic-text [`Metrics`] 的行高——替代原先的
/// `font_size * 1.2` 硬编码近似。
/// 返回 `(width, height)`（px）。
pub(crate) fn measure_text(
    text: &str,
    font_size: f32,
    font_family: &str,
    font_weight: u16,
    line_height: f32,
    max_width: Option<f32>,
    font_system: &mut FontSystem,
) -> (f32, f32) {
    let mut buffer = Buffer::new(font_system, Metrics::new(font_size, line_height));
    // 换行：宽度上限为 Some 则换行，None 则单行。
    buffer.set_size(font_system, max_width, None);
    let attrs = Attrs::new()
        .family(family_from_css(font_family))
        .weight(Weight(font_weight));
    buffer.set_text(font_system, text, attrs, Shaping::Advanced);
    // 遍历 layout runs 计算实际文本尺寸（最大行宽 × 总行高）。`size()` 返回的
    // 是 buffer 设定尺寸而非文本内容尺寸，故这里从 runs 累加。
    // 高度 = 行顶 + 行高：`run.line_y` 是**基线**（含 cosmic-text 的
    // 行内居中偏移 + max_ascent），不是行顶——误用会把文本盒撑到
    // ~2.2em（基线 ≈1em + 行高 1.2em），汉字字形更高、位移更明显。
    let mut width: f32 = 0.0;
    let mut height: f32 = 0.0;
    for run in buffer.layout_runs() {
        width = width.max(run.line_w);
        height = height.max(run.line_top + run.line_height);
    }
    (width, height)
}

/// CSS 字体族名 → cosmic-text [`Family`]。
///
/// 通用族名（serif/sans-serif/monospace/cursive/fantasy）映射到对应变体，
/// 其余按具体字体族名（`Family::Name`）传递。
fn family_from_css(name: &str) -> Family<'_> {
    let trimmed = name.trim();
    match trimmed.to_ascii_lowercase().as_str() {
        "serif" => Family::Serif,
        "sans-serif" => Family::SansSerif,
        "monospace" => Family::Monospace,
        "cursive" => Family::Cursive,
        "fantasy" => Family::Fantasy,
        _ => Family::Name(trimmed),
    }
}

/// 从 ComputedStyle 提取 font-size 的 px 值。
///
/// cascade 已把 font-size 归一化为 px Dimension（`normalize_font_size`），
/// 此处直接解析 `Token::Dimension(_, "px")`。无法解析时返回 `None`
/// （调用方回退到继承的 font-size 或 [`DEFAULT_FONT_SIZE`]）。
/// F-1: 出口经 [`clamp_length`](crate::style_map::clamp_length) 钳制，
/// `font-size: 1e39px` 之类敌意值不得以 inf 进入 cosmic-text `Metrics`。
pub(crate) fn resolve_font_size(style: &ComputedStyle) -> Option<f32> {
    let cv = style.get("font-size")?;
    for v in cv.tokens() {
        if let ComponentValue::PreservedToken(Token::Dimension(numeric, unit)) = v {
            if unit.eq_ignore_ascii_case("px") {
                return Some(crate::style_map::clamp_length(numeric.value));
            }
        }
    }
    None
}

/// 从 ComputedStyle 提取 font-family（取首个字体族名）。
pub(crate) fn resolve_font_family(style: &ComputedStyle) -> Option<String> {
    let cv = style.get("font-family")?;
    cv.tokens().iter().find_map(|t| match t {
        ComponentValue::PreservedToken(Token::Ident(s)) => Some(s.clone()),
        ComponentValue::PreservedToken(Token::String(s)) => Some(s.clone()),
        _ => None,
    })
}

/// 从 ComputedStyle 提取 font-weight（`normal`=400、`bold`=700、数值直接）。
pub(crate) fn resolve_font_weight(style: &ComputedStyle) -> Option<u16> {
    let cv = style.get("font-weight")?;
    for t in cv.tokens() {
        match t {
            ComponentValue::PreservedToken(Token::Ident(s)) => {
                return Some(if s.eq_ignore_ascii_case("bold") {
                    700
                } else {
                    400
                });
            }
            ComponentValue::PreservedToken(Token::Number(n)) => {
                return Some(n.value.clamp(1.0, 1000.0) as u16);
            }
            _ => {}
        }
    }
    None
}
