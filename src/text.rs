//! 文本测量（layout 层）。
//!
//! 用 cosmic-text 测量 text 节点的自然尺寸，供 [`crate::convert::build_layout_tree`]
//! 将 text 节点作为固定尺寸的 taffy leaf 加入布局树（T-1：单行、不换行）。
//!
//! # 规范依据
//!
//! - CSS Fonts Level 3: font-size / line-height
//! - CSS Inline Layout Level 3: inline 盒尺寸（inline 流合并推迟，当前单行）

use cosmic_text::{Attrs, Buffer, FontSystem, Metrics, Shaping};
use muskitty_cascade::ComputedStyle;
use muskitty_css::parser::ComponentValue;
use muskitty_css::tokenizer::Token;

/// 浏览器默认 font-size（px）。CSS `medium` = 16px。
pub(crate) const DEFAULT_FONT_SIZE: f32 = 16.0;

/// 测量单行文本的自然尺寸（不换行）。
///
/// 返回 `(width, height)`（px）。行高按 `font_size * 1.2` 近似（CSS `normal`
/// 行高的简化），精确 line-height 解析推迟。
pub(crate) fn measure_text(text: &str, font_size: f32, font_system: &mut FontSystem) -> (f32, f32) {
    let line_height = font_size * 1.2;
    let mut buffer = Buffer::new(font_system, Metrics::new(font_size, line_height));
    // 单行：不设宽度上限（不触发换行）。
    buffer.set_size(font_system, None, None);
    buffer.set_text(font_system, text, Attrs::new(), Shaping::Advanced);
    // 遍历 layout runs 计算实际文本尺寸（最大行宽 × 总行高）。`size()` 返回的
    // 是 buffer 设定尺寸而非文本内容尺寸，故这里从 runs 累加。
    let mut width: f32 = 0.0;
    let mut height: f32 = 0.0;
    for run in buffer.layout_runs() {
        width = width.max(run.line_w);
        height = height.max(run.line_y + run.line_height);
    }
    (width, height)
}

/// 从 ComputedStyle 提取 font-size 的 px 值。
///
/// cascade 已把 font-size 归一化为 px Dimension（`normalize_font_size`），
/// 此处直接解析 `Token::Dimension(_, "px")`。无法解析时返回 `None`
/// （调用方回退到继承的 font-size 或 [`DEFAULT_FONT_SIZE`]）。
pub(crate) fn resolve_font_size(style: &ComputedStyle) -> Option<f32> {
    let cv = style.get("font-size")?;
    for v in cv.tokens() {
        if let ComponentValue::PreservedToken(Token::Dimension(numeric, unit)) = v {
            if unit.eq_ignore_ascii_case("px") {
                return Some(numeric.value as f32);
            }
        }
    }
    None
}
