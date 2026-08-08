//! DOM → LayoutTree 构建测试。
//!
//! 覆盖：单元素、嵌套元素、Text 节点跳过、display:none 排除。

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use muskitty_cascade::{ComputedStyle, ComputedValue};
use muskitty_dom::{append_child, Node};
use muskitty_layout::build_layout_tree;

/// 创建 Document + 一个指定 tag 的 Element。
fn make_element(tag: &str, doc: &Rc<RefCell<Node>>) -> Rc<RefCell<Node>> {
    Node::new_element_html(tag, vec![], doc)
}

#[test]
fn single_element_builds_one_node() {
    let doc = Node::new_document();
    let root = make_element("div", &doc);

    let styles: HashMap<usize, ComputedStyle> = HashMap::new();
    let tree = build_layout_tree(&root, &styles);

    assert!(tree.root.is_some(), "根节点应被创建");
    assert_eq!(tree.node_map.len(), 1, "单个元素应映射到 1 个布局节点");
}

#[test]
fn nested_elements_build_tree() {
    // div > p > span
    let doc = Node::new_document();
    let root = make_element("div", &doc);
    let p = make_element("p", &doc);
    let span = make_element("span", &doc);
    append_child(&root, Rc::clone(&p)).unwrap();
    append_child(&p, Rc::clone(&span)).unwrap();

    let styles: HashMap<usize, ComputedStyle> = HashMap::new();
    let tree = build_layout_tree(&root, &styles);

    assert_eq!(tree.node_map.len(), 3, "三层嵌套应产生 3 个布局节点");
    assert!(tree.node_map.contains_key(&(Rc::as_ptr(&root) as usize)));
    assert!(tree.node_map.contains_key(&(Rc::as_ptr(&p) as usize)));
    assert!(tree.node_map.contains_key(&(Rc::as_ptr(&span) as usize)));
}

#[test]
fn text_nodes_skipped() {
    // div > "hello"
    let doc = Node::new_document();
    let root = make_element("div", &doc);
    let text = Node::new_text("hello", &doc);
    append_child(&root, text).unwrap();

    let styles: HashMap<usize, ComputedStyle> = HashMap::new();
    let tree = build_layout_tree(&root, &styles);

    assert_eq!(tree.node_map.len(), 1, "Text 节点不应创建布局节点");
}

#[test]
fn display_none_excluded() {
    // div > p[display:none]
    let doc = Node::new_document();
    let root = make_element("div", &doc);
    let p = make_element("p", &doc);
    append_child(&root, Rc::clone(&p)).unwrap();

    let mut styles: HashMap<usize, ComputedStyle> = HashMap::new();
    let p_addr = Rc::as_ptr(&p) as usize;
    let mut p_style = ComputedStyle::new();
    p_style.set("display", ComputedValue::from_keyword("none"));
    styles.insert(p_addr, p_style);

    let tree = build_layout_tree(&root, &styles);

    assert_eq!(tree.node_map.len(), 1, "display:none 的 p 应被排除");
    assert!(
        !tree.node_map.contains_key(&p_addr),
        "p 的地址不应出现在 node_map 中"
    );
}

#[test]
fn display_none_excludes_entire_subtree() {
    // div > p[display:none] > span + em
    let doc = Node::new_document();
    let root = make_element("div", &doc);
    let p = make_element("p", &doc);
    let span = make_element("span", &doc);
    let em = make_element("em", &doc);
    append_child(&root, Rc::clone(&p)).unwrap();
    append_child(&p, Rc::clone(&span)).unwrap();
    append_child(&p, Rc::clone(&em)).unwrap();

    let mut styles: HashMap<usize, ComputedStyle> = HashMap::new();
    let mut p_style = ComputedStyle::new();
    p_style.set("display", ComputedValue::from_keyword("none"));
    styles.insert(Rc::as_ptr(&p) as usize, p_style);

    let tree = build_layout_tree(&root, &styles);

    assert_eq!(tree.node_map.len(), 1, "display:none 的整个子树应被排除");
    assert!(!tree.node_map.contains_key(&(Rc::as_ptr(&span) as usize)));
    assert!(!tree.node_map.contains_key(&(Rc::as_ptr(&em) as usize)));
}

#[test]
fn empty_dom_produces_empty_tree() {
    // Document 节点（非 Element）→ 空树
    let doc = Node::new_document();
    let styles: HashMap<usize, ComputedStyle> = HashMap::new();
    let tree = build_layout_tree(&doc, &styles);

    assert!(tree.root.is_none(), "Document 节点不应产生布局根");
    assert_eq!(tree.node_map.len(), 0);
}

#[test]
fn display_contents_element_produces_no_box_but_children_do() {
    // P2-12: display: contents 元素不生成盒（CSS Display L3 §2.5），其子元素
    // 直接参与祖父格式上下文。
    // div > p[display:contents] > span → div 与 span 生成盒，p 不生成，
    // 且 span 是 div 的直接子盒。
    let doc = Node::new_document();
    let root = make_element("div", &doc);
    let p = make_element("p", &doc);
    let span = make_element("span", &doc);
    append_child(&root, Rc::clone(&p)).unwrap();
    append_child(&p, Rc::clone(&span)).unwrap();

    let mut styles: HashMap<usize, ComputedStyle> = HashMap::new();
    let p_addr = Rc::as_ptr(&p) as usize;
    let span_addr = Rc::as_ptr(&span) as usize;
    let mut p_style = ComputedStyle::new();
    p_style.set("display", ComputedValue::from_keyword("contents"));
    styles.insert(p_addr, p_style);

    let tree = build_layout_tree(&root, &styles);

    assert!(
        !tree.node_map.contains_key(&p_addr),
        "display:contents 元素不应生成盒"
    );
    let root_addr = Rc::as_ptr(&root) as usize;
    assert!(tree.node_map.contains_key(&root_addr));
    assert!(tree.node_map.contains_key(&span_addr));
    let root_id = tree.node_map[&root_addr];
    let span_id = tree.node_map[&span_addr];
    assert!(
        tree.taffy.children(root_id).unwrap().contains(&span_id),
        "display:contents 的子元素应 splice 为祖父的直接子盒"
    );
}

#[test]
fn non_rendered_tags_produce_no_boxes() {
    // P2-13: head/title/script 等非渲染标签不生成布局盒（无 UA 表时）。
    let doc = Node::new_document();
    let root = make_element("html", &doc);
    let head = make_element("head", &doc);
    let title = make_element("title", &doc);
    let script = make_element("script", &doc);
    append_child(&root, Rc::clone(&head)).unwrap();
    append_child(&head, Rc::clone(&title)).unwrap();
    append_child(&head, Rc::clone(&script)).unwrap();

    let styles: HashMap<usize, ComputedStyle> = HashMap::new();
    let tree = build_layout_tree(&root, &styles);

    assert_eq!(tree.node_map.len(), 1, "仅 html 生成盒");
    assert!(tree.node_map.contains_key(&(Rc::as_ptr(&root) as usize)));
    assert!(!tree.node_map.contains_key(&(Rc::as_ptr(&head) as usize)));
    assert!(!tree.node_map.contains_key(&(Rc::as_ptr(&title) as usize)));
    assert!(!tree.node_map.contains_key(&(Rc::as_ptr(&script) as usize)));
}

#[test]
fn contents_element_skips_its_own_display_but_descendants_render() {
    // P2-12: display:contents 的深层子树照常生成盒。
    // div > p[contents] > div[block]
    let doc = Node::new_document();
    let root = make_element("div", &doc);
    let p = make_element("p", &doc);
    let inner = make_element("div", &doc);
    append_child(&root, Rc::clone(&p)).unwrap();
    append_child(&p, Rc::clone(&inner)).unwrap();

    let mut styles: HashMap<usize, ComputedStyle> = HashMap::new();
    let p_addr = Rc::as_ptr(&p) as usize;
    let mut p_style = ComputedStyle::new();
    p_style.set("display", ComputedValue::from_keyword("contents"));
    styles.insert(p_addr, p_style);

    let tree = build_layout_tree(&root, &styles);

    assert!(!tree.node_map.contains_key(&p_addr));
    let inner_addr = Rc::as_ptr(&inner) as usize;
    assert!(
        tree.node_map.contains_key(&inner_addr),
        "contents 的子孙应照常生成盒"
    );
    let root_id = tree.node_map[&(Rc::as_ptr(&root) as usize)];
    let inner_id = tree.node_map[&inner_addr];
    assert!(
        tree.taffy.children(root_id).unwrap().contains(&inner_id),
        "contents 的子子孙应 splice 进祖父 children"
    );
}
