//! 布局树类型定义。
//!
//! [`LayoutTree`] 包装 [`taffy::TaffyTree`]，维护 DOM 节点指针地址 →
//! taffy [`NodeId`] 的映射，供布局计算后按 DOM 节点查询结果。
//!
//! # 规范依据
//!
//! - CSS Display Module Level 3 §2 (Box Tree)
//! - CSS Box Model Module Level 3 §2 (Box Model)

use std::collections::HashMap;

use taffy::NodeId;
use taffy::TaffyTree;

/// 布局树。
///
/// 包装 taffy 的 [`TaffyTree`]，并维护 DOM 节点指针地址（`usize`）
/// 到 taffy [`NodeId`] 的映射。构建阶段由 [`build_layout_tree`](crate::build_layout_tree)
/// 填充；计算阶段由 [`compute_layout`](crate::compute_layout) 读取。
///
/// # key 契约（PERF-12 降级）
///
/// node_map / LayoutResult 的 key 用 `Rc::as_ptr as usize` 裸地址（非不透明
/// 句柄）。已知限制：DOM 树变更后地址可能失效/复用。后续批次应改为每个
/// 元素一个稳定 id 的不透明句柄。
pub struct LayoutTree {
    /// taffy 的内部节点树（默认 context 类型为 `()`）。
    pub taffy: TaffyTree,
    /// DOM 节点指针地址 → taffy NodeId 映射。
    pub node_map: HashMap<usize, NodeId>,
    /// 根节点 ID（若 DOM 根为 Element 且未 display:none 则有值）。
    pub root: Option<NodeId>,
}

impl LayoutTree {
    /// 创建空布局树。
    pub fn new() -> Self {
        Self {
            taffy: TaffyTree::new(),
            node_map: HashMap::new(),
            root: None,
        }
    }

    /// 更新某 DOM 节点对应 taffy 节点的样式。
    ///
    /// PERF-9（降级）：暴露增量更新的入口，供未来复用 [`TaffyTree`] +
    /// `set_style` + 局部 relayout 的增量路径。当前 one-shot pipeline 每帧
    /// 重建整棵布局树，无消费方。样式请用 [`crate::style_map::map_style`] 生成。
    ///
    /// 返回 `None`：addr 不在 node_map 中，或 taffy 内部更新失败。
    pub fn set_style(&mut self, addr: usize, style: taffy::style::Style) -> Option<()> {
        let node = self.node_map.get(&addr)?;
        self.taffy.set_style(*node, style).ok()
    }
}

impl Default for LayoutTree {
    fn default() -> Self {
        Self::new()
    }
}
