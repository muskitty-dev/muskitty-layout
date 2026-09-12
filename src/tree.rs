//! 布局树类型定义。
//!
//! [`LayoutTree`] 包装 [`taffy::TaffyTree`]，维护 DOM 节点指针地址 →
//! taffy [`NodeId`] 的映射，供布局计算后按 DOM 节点查询结果。
//!
//! # 规范依据
//!
//! - CSS Display Module Level 3 §2 (Box Tree)
//! - CSS Box Model Module Level 3 §2 (Box Model)

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

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
        /// 文本内容。M-3 batch 3：`text-transform` 已在此生效（存转换后文本，
        /// 保证测量与绘制看到同一份内容，缓存键也因此自洽）。
        text: String,
        font_size: f32,
        font_family: String,
        font_weight: u16,
        /// `line-height` 的使用值（px，M-3 batch 3；cascade
        /// `text_props::used_line_height_px` 解析，含继承的倍数折算）。
        line_height: f32,
        /// LAY-3：最近一次测量缓存（key = 容器可用宽度，`None` = 不定
        /// /单行；value = 自然尺寸 `(width, height)`）。
        ///
        /// taffy 嵌套布局对同一节点多次调用 measure（文本测量占布局
        /// 耗时 80%+，每节点全量 `Buffer::new` + Advanced shaping）。
        /// 文本/字体参数在节点生命周期内不可变，测量结果仅由可用宽度
        /// 决定——同 key 重复调用直接命中，跳过 shaping。
        measured: Option<(Option<f32>, (f32, f32))>,
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
    ///
    /// LAY-2：`Rc` 共享——由会话级 [`SharedFontSystem`] 注入（多次建树
    /// 复用同一份系统字体枚举），不再每次 [`LayoutTree::new`] 重建。
    pub(crate) font_system: Rc<RefCell<FontSystem>>,
}

/// 会话级共享的字体系统句柄（LAY-2）。
///
/// `FontSystem::new()` 枚举并解析系统字体（50–300 ms）。原实现每次
/// `build_layout_tree` 都随 [`LayoutTree::new`] 新建一份——每次布局、
/// resize、热重载都重复支付。本类型由页面/会话级持有一份，多个
/// [`LayoutTree`] 通过 [`crate::build_layout_tree_with_fonts`] 注入共享
///（`Rc` 引用计数）。
///
/// 包装类型不泄漏 cosmic-text（ADR：外部依赖解耦）。
#[derive(Clone)]
pub struct SharedFontSystem {
    pub(crate) inner: Rc<RefCell<FontSystem>>,
}

impl SharedFontSystem {
    /// 枚举系统字体并构建共享句柄（较慢，每会话一次）。
    pub fn new() -> Self {
        Self {
            inner: Rc::new(RefCell::new(FontSystem::new())),
        }
    }
}

impl Default for SharedFontSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl LayoutTree {
    /// 创建空布局树（内部新建一份 [`FontSystem`]——每次调用支付一次
    /// 系统字体枚举；会话级复用请用 [`LayoutTree::new_with_fonts`]）。
    pub fn new() -> Self {
        Self::new_with_fonts(&SharedFontSystem::new())
    }

    /// 创建空布局树，注入会话级共享字体系统（LAY-2）。
    pub fn new_with_fonts(fonts: &SharedFontSystem) -> Self {
        Self {
            taffy: TaffyTree::new(),
            node_map: HashMap::new(),
            root: None,
            font_system: Rc::clone(&fonts.inner),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// LAY-2：`new_with_fonts` 注入的树共享同一份字体系统（Rc 指向同一
    /// 分配），会话级句柄跨多次建树复用，系统字体只枚举一次。
    #[test]
    fn new_with_fonts_shares_one_font_system() {
        let fonts = SharedFontSystem::new();
        let a = LayoutTree::new_with_fonts(&fonts);
        let b = LayoutTree::new_with_fonts(&fonts);
        assert!(
            Rc::ptr_eq(&a.font_system, &fonts.inner),
            "tree must share the injected font system"
        );
        assert!(
            Rc::ptr_eq(&a.font_system, &b.font_system),
            "two trees from one handle must share one font system"
        );
    }

    /// LAY-2：`LayoutTree::new`（兼容路径）自建字体系统，树间互不共享。
    #[test]
    fn new_owns_fresh_font_system() {
        let a = LayoutTree::new();
        let b = LayoutTree::new();
        assert!(!Rc::ptr_eq(&a.font_system, &b.font_system));
    }
}
