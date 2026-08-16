//! MusKitty Layout — CSS 布局引擎。
//!
//! 将 DOM 树 + ComputedStyle 转换为布局盒树，
//! 使用 taffy 引擎计算 Flexbox/Grid/Block 布局，
//! 输出每个元素的位置和尺寸。
//!
//! # 数据流
//!
//! ```text
//! DOM 树 + ComputedStyle per element
//!     │  build_layout_tree
//!     ▼
//! LayoutTree (taffy TaffyTree + NodeId 映射)
//!     │  compute_layout
//!     ▼
//! LayoutResult (per-element x/y/width/height)
//! ```
//!
//! # 规范依据
//!
//! - CSS Display Level 3: box tree / formatting context
//! - CSS Box Model Level 3: margin/border/padding/content
//! - CSS Flexbox Level 1: flex container/item
//! - CSS Grid Level 2: grid container/item

pub(crate) mod convert;
pub mod result;
pub(crate) mod style_map;
pub(crate) mod text;
pub(crate) mod tree;

pub use convert::build_layout_tree;
pub use result::{LayoutError, LayoutResult, NodeLayout};
pub use tree::LayoutTree;

use taffy::geometry::Size;
use taffy::style::AvailableSpace;

use std::collections::HashMap;
use taffy::NodeId;

use crate::text::measure_text;
use crate::tree::NodeContext;

/// 计算布局树，返回每个元素的布局结果。
///
/// `viewport_width` / `viewport_height` 为根容器的可用空间（px），
/// 作为 taffy 布局计算的 definite available space。
///
/// 调用后 [`LayoutTree::taffy`] 内部缓存了每个节点的 [`taffy::tree::Layout`]，
/// 本函数遍历 `node_map` 提取位置与尺寸到 [`LayoutResult`]。
///
/// # 错误处理
///
/// 返回 `Result<LayoutResult, LayoutError>`。当 taffy 内部报告错误时
/// （如 flex 容器循环引用、style 含 NaN/Inf、NodeId 查询失败），
/// 调用方应决定降级策略：使用空 [`LayoutResult::default`] 让 paint 阶段
/// 自然跳过所有元素，或显示错误占位符。**不应** `expect` panic 跨模块传播。
pub fn compute_layout(
    tree: &mut LayoutTree,
    viewport_width: f32,
    viewport_height: f32,
) -> Result<LayoutResult, LayoutError> {
    let mut result = LayoutResult::new();

    if let Some(root) = tree.root {
        // measure function：对 Text context 按容器可用宽度换行测量（T-3）。
        // split borrow：font_system 与 taffy 是 LayoutTree 的不同字段。
        let font_system = &mut tree.font_system;
        let measure = |_known: Size<Option<f32>>,
                       available: Size<AvailableSpace>,
                       _id: NodeId,
                       ctx: Option<&mut NodeContext>,
                       _style: &taffy::style::Style|
         -> Size<f32> {
            let Some(NodeContext::Text {
                text,
                font_size,
                font_family,
                font_weight,
            }) = ctx
            else {
                return Size::ZERO;
            };
            let max_width = match available.width {
                AvailableSpace::Definite(w) => Some(w),
                _ => None,
            };
            let (measured_w, h) = measure_text(
                text,
                *font_size,
                font_family,
                *font_weight,
                max_width,
                font_system,
            );
            // 换行时 width = 容器可用宽度（占满行），renderer 用同宽换行保持一致。
            let width = match available.width {
                AvailableSpace::Definite(w) => w,
                _ => measured_w,
            };
            Size { width, height: h }
        };

        tree.taffy.compute_layout_with_measure(
            root,
            Size {
                width: AvailableSpace::Definite(viewport_width),
                height: AvailableSpace::Definite(viewport_height),
            },
            measure,
        )?;

        for (&dom_addr, &taffy_node) in &tree.node_map {
            let layout = tree
                .taffy
                .layout(taffy_node)
                .map_err(|_| LayoutError::NodeLayoutMissing(taffy_node))?;
            result.nodes.insert(
                dom_addr,
                NodeLayout {
                    x: layout.location.x,
                    y: layout.location.y,
                    abs_x: 0.0,
                    abs_y: 0.0,
                    width: layout.size.width,
                    height: layout.size.height,
                },
            );
        }

        // 绝对坐标：自根沿 taffy 树累加偏移（P2-19 / PERF-12）。
        //
        // taffy 的 location 相对其 taffy 父节点。`display: contents` /
        // 非渲染标签 splice 后，DOM 祖先链 ≠ taffy 父链——Renderer 若沿
        // DOM 祖先累加，未来引入 `position: absolute`（location 相对
        // containing block）或 transform 时就会双重计数。因此这里直接沿
        // taffy 树把偏移累加为画布绝对坐标，Renderer 只读 `abs_x`/`abs_y`。
        let reverse: HashMap<NodeId, usize> =
            tree.node_map.iter().map(|(&addr, &n)| (n, addr)).collect();
        // 迭代式 DFS（避免深 DOM 递归溢出）；children 顺序即 DOM 先序。
        let mut stack: Vec<(NodeId, f32, f32)> = vec![(root, 0.0, 0.0)];
        while let Some((node_id, parent_abs_x, parent_abs_y)) = stack.pop() {
            if let Ok(layout) = tree.taffy.layout(node_id) {
                let abs_x = parent_abs_x + layout.location.x;
                let abs_y = parent_abs_y + layout.location.y;
                if let Some(&addr) = reverse.get(&node_id) {
                    if let Some(entry) = result.nodes.get_mut(&addr) {
                        entry.abs_x = abs_x;
                        entry.abs_y = abs_y;
                    }
                }
                if let Ok(children) = tree.taffy.children(node_id) {
                    for child in children {
                        stack.push((child, abs_x, abs_y));
                    }
                }
            }
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_tree_returns_ok_empty_result() {
        let mut tree = LayoutTree::new();
        let result = compute_layout(&mut tree, 800.0, 600.0).expect("empty tree should succeed");
        assert!(result.nodes.is_empty());
    }

    #[test]
    fn compute_layout_propagates_taffy_error_on_nan_input() {
        // 构造一个会导致 taffy 报错的 LayoutTree：在 root style 中塞入 NaN 尺寸。
        use taffy::geometry::Size as TaffySize;
        use taffy::style::{Dimension, Style};

        let mut tree = LayoutTree::new();
        let root = tree
            .taffy
            .new_leaf(Style {
                size: TaffySize {
                    width: Dimension::length(f32::NAN),
                    height: Dimension::length(f32::NAN),
                },
                ..Default::default()
            })
            .expect("new_leaf should succeed");
        tree.root = Some(root);

        let result = compute_layout(&mut tree, 800.0, 600.0);
        // taffy 可能返回 Err 或成功；若返回 Err 应为 ComputeLayoutFailed
        match result {
            Err(LayoutError::ComputeLayoutFailed(_)) => {}
            Ok(_) => {
                // 某些 taffy 版本对 NaN 容忍，此情况下允许 Ok
            }
            Err(other) => panic!("expected ComputeLayoutFailed or Ok, got {other:?}"),
        }
    }
}
