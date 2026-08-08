//! 布局计算结果提取与错误类型。
//!
//! [`LayoutResult`] 是 [`compute_layout`](crate::compute_layout) 的输出，
//! 按 DOM 节点指针地址索引每个元素的位置与尺寸。
//!
//! [`LayoutError`] 是 [`compute_layout`](crate::compute_layout) 可能返回的错误，
//! 用于替代旧的 `expect` panic，便于上层应用做降级处理（如跳过该元素、显示错误占位符）。

use std::collections::HashMap;

use taffy::NodeId;

/// 单个元素的布局结果。
///
/// `x` / `y` 是相对 taffy 父节点原点的偏移（px）；`abs_x` / `abs_y` 是画布
/// 坐标系（视口左上角原点）的绝对坐标，由 [`compute_layout`](crate::compute_layout)
/// 沿 taffy 树自根累加得到（P2-19）。
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct NodeLayout {
    /// 相对 taffy 父节点原点的 X 坐标（px）。
    pub x: f32,
    /// 相对 taffy 父节点原点的 Y 坐标（px）。
    pub y: f32,
    /// 画布坐标系绝对 X 坐标（px）。
    pub abs_x: f32,
    /// 画布坐标系绝对 Y 坐标（px）。
    pub abs_y: f32,
    /// 计算后的宽度（px）。
    pub width: f32,
    /// 计算后的高度（px）。
    pub height: f32,
}

/// 布局计算错误。
///
/// 所有错误均来自 taffy 内部，通常由 NaN/Inf 输入、循环 flex 引用、
/// 或不存在的 NodeId 查询触发。调用方应通过 `Result` 处理，
/// 而非依赖 `expect` panic 跨模块传播。
#[derive(Debug)]
pub enum LayoutError {
    /// taffy `compute_layout` 失败。常见原因：flex 容器循环引用、
    /// style 中包含 NaN/Inf、taffy 内部断言失败。
    ComputeLayoutFailed(taffy::TaffyError),
    /// `taffy.layout(node)` 查询失败：节点不在布局树中。
    /// 通常由 `node_map` 与 taffy 内部状态不同步导致。
    NodeLayoutMissing(NodeId),
}

impl std::fmt::Display for LayoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LayoutError::ComputeLayoutFailed(e) => {
                write!(f, "taffy compute_layout failed: {e}")
            }
            LayoutError::NodeLayoutMissing(id) => {
                write!(f, "taffy layout missing for node {id:?}")
            }
        }
    }
}

impl std::error::Error for LayoutError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            LayoutError::ComputeLayoutFailed(e) => Some(e),
            LayoutError::NodeLayoutMissing(_) => None,
        }
    }
}

impl From<taffy::TaffyError> for LayoutError {
    fn from(e: taffy::TaffyError) -> Self {
        LayoutError::ComputeLayoutFailed(e)
    }
}

/// 整棵布局树的结果集合。
///
/// 通过 DOM 节点指针地址（`usize`）查询对应元素的 [`NodeLayout`]。
///
/// key 用 `Rc::as_ptr as usize` 裸地址（PERF-12 降级：未做不透明句柄）。
/// 已知限制：DOM 树变更后地址可能失效/复用；后续批次应改为不透明句柄
/// （如每个元素一个稳定 id）或按 [`taffy::NodeId`] 的树形访问。
#[derive(Debug, Default)]
pub struct LayoutResult {
    /// DOM 节点指针地址 → 布局结果。
    pub nodes: HashMap<usize, NodeLayout>,
}

impl LayoutResult {
    /// 创建空结果集。
    pub fn new() -> Self {
        Self::default()
    }

    /// 按 DOM 节点指针地址查询布局结果。
    pub fn get(&self, node_addr: usize) -> Option<&NodeLayout> {
        self.nodes.get(&node_addr)
    }
}
