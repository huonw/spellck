use std::collections::{TreeMap, HashSet};
use std::ascii::AsciiExt;

use syntax::{ast, visit};
use syntax::parse::token;
use syntax::codemap::{Span, BytePos};
use syntax::attr::AttrMetaMethods;
use syntax::ast::NodeId;
use syntax::ast_util::PostExpansionMethod;

use rustc::middle::privacy::ExportedItems;

use words;
use stem;

pub struct Position {
    pub span: Span,
    pub id: NodeId,
}

impl Position {
    fn new(sp: Span, id: NodeId) -> Position {
        Position { span: sp, id: id, }
    }
}

impl PartialEq for Position {
    fn eq(&self, other: &Position) -> bool {
        self.cmp(other) == Equal
    }
}
impl Eq for Position {}
impl PartialOrd for Position {
    fn partial_cmp(&self, other: &Position) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Position {
    fn cmp(&self, other: &Position) -> Ordering {
        let Span { lo: BytePos(slo), hi: BytePos(shi), .. } = self.span;
        let Span { lo: BytePos(olo), hi: BytePos(ohi), .. } = other.span;
        // order by span, and then by ID.
        (slo, shi, self.id).cmp(&(olo, ohi, other.id))
    }
}

/// Keeps track of the reference dictionary and the misspelled words
/// through a traversal of the whole ast.
pub struct SpellingVisitor<'a> {
    /// The reference dictionary.
    words: &'a HashSet<String>,

    /// The truly exported items.
    exported: &'a ExportedItems,

    /// The misspelled words
    pub misspellings: TreeMap<Position, Vec<String>>,

    /// Whether the traversal should only check documentation, not
    /// idents; gets controlled internally, e.g. for `extern` blocks.
    doc_only: bool
}

impl<'a> SpellingVisitor<'a> {
    /// ast::Create a new Spelling Visitor.
    pub fn new<'a>(words: &'a HashSet<String>,
                   exported: &'a ExportedItems) -> SpellingVisitor<'a> {
        SpellingVisitor {
            words: words,
            exported: exported,
            misspellings: TreeMap::new(),
            doc_only: false
        }
    }

    /// Checks if the given string is a correct "word", without
    /// splitting it at all. Any word that isn't entirely alphabetic
    /// is automatically considered a proper word.
    fn raw_word_is_correct(&mut self, w: &str) -> bool {
        self.words.contains_equiv(&w) ||
            !w.chars().all(|c| c.is_alphabetic()) ||
            self.words.contains_equiv(&w.to_ascii_lower()) ||
            self.stemmed_word_is_correct(w)
    }

    fn stemmed_word_is_correct(&self, w: &str) -> bool {
        stem::get(w).ok().map_or(false,
            |s| self.words.contains_equiv(&s.as_slice().to_ascii_lower()))
    }

    /// Check a word for correctness, including splitting `foo_bar`
    /// and `FooBar` into `foo` & `bar` and `Foo` & `Bar`
    /// respectively. This inserts any incorrect word(s) into the
    /// misspelling map.
    fn check_subwords(&mut self, w: &str, pos: Position) {
        for w in words::subwords(w) {
            if !self.raw_word_is_correct(w) {
                let w = w.to_string();
                match self.misspellings.find_mut(&pos) {
                    Some(v) => {
                        v.push(w);
                        continue
                    }
                    None => {}
                }
                self.misspellings.insert(pos, vec![w]);
            }
        }
    }

    /// Check a single ident for misspellings; possibly separating it
    /// into subwords.
    fn check_ident(&mut self, ident: ast::Ident, pos: Position) {
        if self.doc_only { return }

        // spooky action at a distance; extracts the string
        // representation from TLS.
        let word_ = token::get_ident(ident);
        let word = word_.get();
        // secret rust internals, e.g. __std_macros
        if word.starts_with("__") { return }

        // the ident itself is correct, so shortcircuit to avoid doing
        // any of the submatching done below.
        if self.raw_word_is_correct(word) {
            return
        }

        self.check_subwords(word, pos);
    }

    /// Check the #[doc="..."] (and the commment forms) attributes for
    /// spelling.
    fn check_doc_attrs(&mut self, attrs: &[ast::Attribute], id: NodeId) {
        for attr in attrs.iter() {
            if attr.check_name("doc") {
                match attr.value_str() {
                    Some(s) => self.check_subwords(s.get(), Position::new(attr.span, id)),
                    None => {}
                }
            }
        }
    }

    /// Spell-check a whole krate.
    pub fn check_crate(&mut self, krate: &ast::Crate) {
        self.check_doc_attrs(krate.attrs.as_slice(), ast::CRATE_NODE_ID);
        visit::walk_crate(self, krate)
    }
}

// visits anything that could be visible to the outside world,
// e.g. documentation, pub fns, pub mods etc and checks their
// spelling.
impl<'a, 'v> visit::Visitor<'v> for SpellingVisitor<'a> {
    fn visit_view_item(&mut self, view_item: &ast::ViewItem) {
        // only check the ident for `use self = foo;`; since there's
        // nothing else the user can do to control the name.
        if view_item.vis == ast::Public {
            // FIXME #6: no node ids
            // self.check_doc_attrs(view_item.attrs.as_slice());
            match view_item.node {
                ast::ViewItemUse(ref vp) => {
                    match vp.node {
                        ast::ViewPathSimple(_ident, _, _) => {
                            // self.check_ident(id, vp.span);
                        }
                        _ => {}
                    }
                }
                ast::ViewItemExternCrate(..) => {}
            }
        }
    }
    fn visit_foreign_item(&mut self, foreign_item: &ast::ForeignItem) {
        if self.exported.contains(&foreign_item.id) {
            // don't check the ident; there's nothing the user can do to
            // control the name.
            self.check_doc_attrs(foreign_item.attrs.as_slice(), foreign_item.id);
        }
    }

    fn visit_item(&mut self, item: &ast::Item) {
        let is_impl = match item.node {
            ast::ItemImpl(..) => true,
            _ => false
        };
        let is_exported = self.exported.contains(&item.id);

        // checking names in impl headers is pointless: they're declared elsewhere.
        if is_exported && !is_impl {
            self.check_ident(item.ident, Position::new(item.span, item.id));
        }
        if is_exported {
            self.check_doc_attrs(item.attrs.as_slice(), item.id);
        }

        match item.node {
            // no visitor method for enum variants so have to do it by
            // hand. This is probably (subtly or otherwise) incorrect
            // wrt to visibility.
            ast::ItemEnum(ref ed, _) => {
                for var in ed.variants.iter() {
                    if self.exported.contains(&var.node.id) {
                        self.check_ident(var.node.name, Position::new(var.span, var.node.id));
                        self.check_doc_attrs(var.node.attrs.as_slice(), var.node.id);
                    }
                }
            }
            ast::ItemMod(..) | ast::ItemForeignMod(..) | ast::ItemStruct(..) => {
                visit::walk_item(self, item)
            }
            // impl Type { ... }
            ast::ItemImpl(_, None, _, ref items) => {
                for item in items.iter() {
                    match *item {
                        ast::MethodImplItem(ref method) => {
                            if self.exported.contains(&method.id) {
                                self.check_ident(method.pe_ident(),
                                                 Position::new(method.span, method.id));
                                self.check_doc_attrs(method.attrs.as_slice(), method.id);
                            }
                        }
                        ast::TypeImplItem(ref item) => {
                            if self.exported.contains(&item.id) {
                                self.check_ident(item.ident,
                                                 Position::new(item.span, item.id));
                                self.check_doc_attrs(item.attrs.as_slice(), item.id);
                            }
                        }
                    }
                }
            }
            // impl Trait for Type { ... }, only check the docs, the
            // method names come from elsewhere.
            ast::ItemImpl(_, Some(..), _, _) => {
                let old_d_o = self.doc_only;
                self.doc_only = true;
                visit::walk_item(self, item);
                self.doc_only = old_d_o;
            }
            ast::ItemTrait(..) if item.vis == ast::Public => {
                visit::walk_item(self, item)
            }
            _ => {}
        }
    }

    fn visit_ty_method(&mut self, method_type: &ast::TypeMethod) {
        self.check_doc_attrs(method_type.attrs.as_slice(), method_type.id);
        self.check_ident(method_type.ident, Position::new(method_type.span, method_type.id));
        visit::walk_ty_method(self, method_type)
    }
    fn visit_trait_item(&mut self, trait_item: &ast::TraitItem) {
        match *trait_item {
            ast::RequiredMethod(_) => {
                visit::walk_trait_item(self, trait_item)
            }
            ast::ProvidedMethod(ref method) => {
                self.check_doc_attrs(method.attrs.as_slice(), method.id);
                self.check_ident(method.pe_ident(), Position::new(method.span, method.id));
            }
            ast::TypeTraitItem(ref item) => {
                self.check_doc_attrs(item.attrs.as_slice(), item.id);
                self.check_ident(item.ident, Position::new(item.span, item.id));
            }
        }
    }

    fn visit_struct_def(&mut self,
                        struct_definition: &ast::StructDef,
                        _identifier: ast::Ident,
                        _generics: &ast::Generics,
                        _node_id: ast::NodeId) {
        visit::walk_struct_def(self,
                               struct_definition)
    }
    fn visit_struct_field(&mut self, struct_field: &ast::StructField) {
        match struct_field.node.kind {
            ast::NamedField(ident, vis) => {
                match vis {
                    ast::Public => {
                        self.check_ident(ident,
                                         Position::new(struct_field.span, struct_field.node.id));
                        self.check_doc_attrs(struct_field.node.attrs.as_slice(),
                                             struct_field.node.id);
                    }
                    ast::Inherited => {}
                }
            }
            ast::UnnamedField(_) => {}
        }

        // no need to recur; nothing below this level to check.
    }

    /// we're only interested in top-level things, so we can just
    /// ignore these entirely.
    fn visit_local(&mut self, _local: &ast::Local) {}
    fn visit_block(&mut self, _block: &ast::Block) {}
    fn visit_stmt(&mut self, _statement: &ast::Stmt) {}
    fn visit_arm(&mut self, _arm: &ast::Arm) {}
    fn visit_pat(&mut self, _pattern: &ast::Pat) {}
    fn visit_decl(&mut self, _declaration: &ast::Decl) {}
    fn visit_expr(&mut self, _expression: &ast::Expr) {}
    fn visit_expr_post(&mut self, _expression: &ast::Expr) {}
    fn visit_ty(&mut self, _typ: &ast::Ty) {}
    fn visit_generics(&mut self, _generics: &ast::Generics) {}
    fn visit_fn(&mut self,
                _function_kind: visit::FnKind,
                _function_declaration: &ast::FnDecl,
                _block: &ast::Block,
                _span: Span,
                _node_id: ast::NodeId) {}
}
