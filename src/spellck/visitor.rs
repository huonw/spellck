use std::collections::{BTreeMap, HashSet};
use std::ascii::AsciiExt;
use std::cmp::Ordering;

use syntax::{ast, visit};
use syntax::parse::token;
use syntax::codemap::{Span, BytePos};
use syntax::attr::AttrMetaMethods;
use syntax::ast::NodeId;

use rustc::middle::privacy::ExportedItems;

use words;
use stem;

#[derive(Copy)]
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
        self.cmp(other) == Ordering::Equal
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
    pub misspellings: BTreeMap<Position, Vec<String>>,

    /// Whether the traversal should only check documentation, not
    /// idents; gets controlled internally, e.g. for `extern` blocks.
    doc_only: bool
}

impl<'a> SpellingVisitor<'a> {
    /// ast::Create a new Spelling Visitor.
    pub fn new<'b>(words: &'b HashSet<String>,
                   exported: &'b ExportedItems) -> SpellingVisitor<'b> {
        SpellingVisitor {
            words: words,
            exported: exported,
            misspellings: BTreeMap::new(),
            doc_only: false
        }
    }

    /// Checks if the given string is a correct "word", without
    /// splitting it at all. Any word that isn't entirely alphabetic
    /// is automatically considered a proper word.
    fn raw_word_is_correct(&mut self, w: &str) -> bool {
        self.words.contains(w) ||
            (w.chars().all(|c| c.is_alphabetic()) && {
                let lower = w.to_ascii_lowercase();
                self.words.contains(&lower) ||
                self.stemmed_word_is_correct(&lower)
            })
    }

    fn stemmed_word_is_correct(&self, w: &str) -> bool {
        stem::get(w).ok().map_or(false, |s| self.words.contains(&s))
    }

    /// Check a word for correctness, including splitting `foo_bar`
    /// and `FooBar` into `foo` & `bar` and `Foo` & `Bar`
    /// respectively. This inserts any incorrect word(s) into the
    /// misspelling map.
    fn check_subwords(&mut self, w: &str, pos: Position) {
        for w in words::subwords(w) {
            if !self.raw_word_is_correct(w) {
                let w = w.to_string();
                self.misspellings.entry(pos).get().unwrap_or_else(|v| v.insert(vec![])).push(w);
            }
        }
    }

    /// Check a single ident for misspellings; possibly separating it
    /// into subwords.
    fn check_ident(&mut self, ident: ast::Ident, pos: Position) {
        if self.doc_only { return }

        // spooky action at a distance; extracts the string
        // representation from TLS.
        let word = token::get_ident(ident);
        // secret rust internals, e.g. __std_macros
        if word.starts_with("__") { return }

        // the ident itself is correct, so shortcircuit to avoid doing
        // any of the submatching done below.
        if self.raw_word_is_correct(&word) {
            return
        }

        self.check_subwords(&word, pos);
    }

    /// Check the #[doc="..."] (and the commment forms) attributes for
    /// spelling.
    fn check_doc_attrs(&mut self, attrs: &[ast::Attribute], id: NodeId) {
        for attr in attrs.iter() {
            if attr.check_name("doc") {
                match attr.value_str() {
                    Some(s) => self.check_subwords(&s, Position::new(attr.span, id)),
                    None => {}
                }
            }
        }
    }

    /// Spell-check a whole krate.
    pub fn check_crate(&mut self, krate: &ast::Crate) {
        self.check_doc_attrs(&krate.attrs, ast::CRATE_NODE_ID);
        visit::walk_crate(self, krate)
    }
}

// visits anything that could be visible to the outside world,
// e.g. documentation, pub fns, pub mods etc and checks their
// spelling.
impl<'a, 'v> visit::Visitor<'v> for SpellingVisitor<'a> {
    fn visit_foreign_item(&mut self, foreign_item: &ast::ForeignItem) {
        if self.exported.contains(&foreign_item.id) {
            // don't check the ident; there's nothing the user can do to
            // control the name.
            self.check_doc_attrs(&foreign_item.attrs, foreign_item.id);
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
            self.check_doc_attrs(&item.attrs, item.id);
        }

        match item.node {
            // no visitor method for enum variants so have to do it by
            // hand. This is probably (subtly or otherwise) incorrect
            // wrt to visibility.
            ast::ItemEnum(ref ed, _) => {
                for var in ed.variants.iter() {
                    if self.exported.contains(&var.node.id) {
                        self.check_ident(var.node.name, Position::new(var.span, var.node.id));
                        self.check_doc_attrs(&var.node.attrs, var.node.id);
                    }
                }
            }
            ast::ItemMod(..) | ast::ItemForeignMod(..) | ast::ItemStruct(..) => {
                visit::walk_item(self, item)
            }
            // impl Type { ... }
            ast::ItemImpl(_, _, _, ref trait_, _, ref items) => {
                let is_trait = trait_.is_some();
                for item in items.iter() {
                    self.check_doc_attrs(&item.attrs, item.id);
                    if !is_trait {
                        // name comes from the trait
                        self.check_ident(item.ident, Position::new(item.span, item.id));
                    }
                }
            }
            ast::ItemTrait(..) if item.vis == ast::Public => {
                visit::walk_item(self, item)
            }
            _ => {}
        }
    }

    fn visit_trait_item(&mut self, trait_item: &ast::TraitItem) {
        self.check_doc_attrs(&trait_item.attrs, trait_item.id);
        self.check_ident(trait_item.ident, Position::new(trait_item.span, trait_item.id));
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
                        self.check_doc_attrs(&struct_field.node.attrs,
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
