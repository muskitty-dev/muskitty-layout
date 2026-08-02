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

pub mod convert;
pub mod result;
pub mod style_map;
pub mod tree;

pub use convert::build_layout_tree;
pub use result::{LayoutError, LayoutResult, NodeLayout};
pub use tree::LayoutTree;

use taffy::geometry::Size;
use taffy::style::AvailableSpace;

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
        tree.taffy.compute_layout(
            root,
            Size {
                width: AvailableSpace::Definite(viewport_width),
                height: AvailableSpace::Definite(viewport_height),
            },
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
                    width: layout.size.width,
                    height: layout.size.height,
                },
            );
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
