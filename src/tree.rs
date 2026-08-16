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

use cosmic_text::FontSystem;
use taffy::NodeId;
use taffy::TaffyTree;

/// taffy 节点的 context，用于 text 节点的 measure function（T-3 换行）。
///
/// 仅 Text 节点携带 context；非 text 节点 measure function 收到的
/// `Option<&mut NodeContext>` 为 `None`。
pub(crate) enum NodeContext {
    /// Text 节点：携带文本内容 + 字体样式，布局时由 measure function 按
    /// 容器可用宽度换行测量。
    Text {
        text: String,
        font_size: f32,
        font_family: String,
        font_weight: u16,
    },
}

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
    /// taffy 的内部节点树（context 类型为 [`NodeContext`]）。
    pub(crate) taffy: TaffyTree<NodeContext>,
    /// DOM 节点指针地址 → taffy NodeId 映射。
    pub(crate) node_map: HashMap<usize, NodeId>,
    /// 根节点 ID（若 DOM 根为 Element 且未 display:none 则有值）。
    pub(crate) root: Option<NodeId>,
    /// 文本测量用的字体系统（compute_layout 的 measure function 使用）。
    pub(crate) font_system: FontSystem,
}

impl LayoutTree {
    /// 创建空布局树。
    pub fn new() -> Self {
        Self {
            taffy: TaffyTree::new(),
            node_map: HashMap::new(),
            root: None,
            font_system: FontSystem::new(),
        }
    }

    // —— #[doc(hidden)] 测试辅助：对外只暴露抽象行为，隐藏 taffy 类型 ——

    /// 是否有根节点。
    #[doc(hidden)]
    pub fn has_root(&self) -> bool {
        self.root.is_some()
    }

    /// 布局节点数量。
    #[doc(hidden)]
    pub fn node_count(&self) -> usize {
        self.node_map.len()
    }

    /// 某 DOM 地址是否在布局树中。
    #[doc(hidden)]
    pub fn contains_node(&self, addr: usize) -> bool {
        self.node_map.contains_key(&addr)
    }

    /// `parent` 是否为 `child` 的 taffy 父节点（验证 contents splice 等）。
    #[doc(hidden)]
    pub fn has_child(&self, parent: usize, child: usize) -> bool {
        let (Some(&p), Some(&c)) = (self.node_map.get(&parent), self.node_map.get(&child)) else {
            return false;
        };
        self.taffy
            .children(p)
            .map(|cs| cs.contains(&c))
            .unwrap_or(false)
    }
}

impl Default for LayoutTree {
    fn default() -> Self {
        Self::new()
    }
}
