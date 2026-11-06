use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::Error;
use crate::lexer::{Token, TokenKind, lex};
use crate::parser::ast::{
    BinaryOp, Callee, Expr, Method, Parameter, Stmt, StmtKind, StructField, TraitMethod, TypeParameter,
    UnaryOp,
};

pub struct Parser {
    tokens: Vec<Token>,
    current: usize,
    macros: HashMap<String, MacroDef>,
    macro_depth: usize,
    /// The file being parsed. `None` means an in-memory snippet such as a REPL
    /// line or a test.
    current_file: Option<PathBuf>,
    /// Directory used to resolve relative imports.
    base_dir: PathBuf,
    /// Directory containing the running interpreter, searched before the
    /// importing file's directory for bare imports.
    install_dir: Option<PathBuf>,
    /// Import paths currently being parsed, for cycle detection.
    loading: HashSet<PathBuf>,
}

#[derive(Clone, Debug)]
struct MacroDef {
    params: Vec<String>,
    body: Vec<Token>,
}

impl Parser {
    pub fn parse(source: &str) -> Result<Vec<Stmt>, Error> {
        let tokens = lex(source)?;
        let base_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let mut parser = Parser {
            tokens,
            current: 0,
            macros: HashMap::new(),
            macro_depth: 0,
            current_file: None,
            base_dir,
            install_dir: installation_dir(),
            loading: HashSet::new(),
        };
        parser.program()
    }

    /// Parses a script from disk, resolving its imports relative to the file.
    pub fn parse_file(path: &Path) -> Result<Vec<Stmt>, Error> {
        Self::parse_file_with_install_dir(path, installation_dir())
    }

    fn parse_file_with_install_dir(
        path: &Path,
        install_dir: Option<PathBuf>,
    ) -> Result<Vec<Stmt>, Error> {
        let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let display = canonical.display().to_string();
        let source = fs::read_to_string(&canonical)
            .map_err(|error| Error::new(1, format!("cannot read '{display}': {error}")))?;
        let tokens = lex(&source).map_err(|error| error.with_file(display.clone()))?;
        let base_dir = canonical
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();

        let mut parser = Parser {
            tokens,
            current: 0,
            macros: HashMap::new(),
            macro_depth: 0,
            current_file: Some(canonical.clone()),
            base_dir,
            install_dir,
            loading: HashSet::from([canonical.clone()]),
        };

        parser.program().map_err(|error| error.with_file(display))
    }

    fn program(&mut self) -> Result<Vec<Stmt>, Error> {
        self.skip_separators();

        let mut statements: Vec<Stmt> = Vec::new();
        while !self.check(&TokenKind::Eof) {
            statements.push(self.statement()?);
            self.end_statement()?;
            self.skip_separators();
        }

        Ok(statements)
    }

    fn statement(&mut self) -> Result<Stmt, Error> {
        let line = self.peek().line;

        match self.peek().kind.clone() {
            TokenKind::Del => {
                self.advance();
                let target = self.expression()?;

                let kind = match target {
                    Expr::Var(name) => StmtKind::Delete { name },
                    Expr::Index { object, index } => {
                        let Expr::Var(name) = *object else {
                            return Err(Error::new(
                                line,
                                "only a variable lookup or map/list entry can be deleted",
                            ));
                        };
                        StmtKind::DeleteIndex {
                            name,
                            index: *index,
                        }
                    }
                    Expr::Slice {
                        object,
                        start,
                        stop,
                        step,
                    } => {
                        let Expr::Var(name) = *object else {
                            return Err(Error::new(
                                line,
                                "only a variable list can be slice-deleted",
                            ));
                        };
                        StmtKind::DeleteSlice {
                            name,
                            start: start.map(|expr| *expr),
                            stop: stop.map(|expr| *expr),
                            step: step.map(|expr| *expr),
                        }
                    }
                    _ => {
                        return Err(Error::new(
                            line,
                            "expected a variable, index, or slice after 'del'",
                        ));
                    }
                };

                Ok(Stmt { kind, line })
            }
            TokenKind::Def => {
                self.advance();
                self.function_statement(line)
            }
            TokenKind::Struct => {
                self.advance();
                self.struct_statement(line)
            }
            TokenKind::Impl => {
                self.advance();
                self.impl_statement(line)
            }
            TokenKind::Trait => {
                self.advance();
                self.trait_statement(line)
            }
            TokenKind::Macro => {
                self.advance();
                self.macro_definition(line)
            }
            TokenKind::Import => {
                self.advance();
                self.import_statement(line)
            }
            TokenKind::Return => {
                self.advance();
                self.return_statement(line)
            }
            TokenKind::If => {
                self.advance();
                self.if_statement(line)
            }
            TokenKind::While => {
                self.advance();
                let condition = self.expression()?;
                let body = self.block()?;
                Ok(Stmt {
                    kind: StmtKind::While { condition, body },
                    line,
                })
            }
            TokenKind::Loop => {
                self.advance();
                let body = self.block()?;
                Ok(Stmt {
                    kind: StmtKind::Loop { body },
                    line,
                })
            }
            TokenKind::For => {
                self.advance();
                let mut names = vec![self.consume_identifier(
                    "expected a loop variable after 'for'",
                )?];

                if self.match_kind(&TokenKind::Comma) {
                    names.push(self.consume_identifier(
                        "expected a second loop variable after ','",
                    )?);

                    if self.check(&TokenKind::Comma) {
                        return Err(Error::new(
                            line,
                            "a 'for' loop takes at most two loop variables",
                        ));
                    }
                }

                self.consume(&TokenKind::In, "expected 'in' after the loop variable")?;
                let iterable = self.expression()?;
                let body = self.block()?;
                Ok(Stmt {
                    kind: StmtKind::For {
                        names,
                        iterable,
                        body,
                    },
                    line,
                })
            }
            TokenKind::Break => {
                self.advance();
                Ok(Stmt {
                    kind: StmtKind::Break,
                    line,
                })
            }
            TokenKind::Continue => {
                self.advance();
                Ok(Stmt {
                    kind: StmtKind::Continue,
                    line,
                })
            }
            TokenKind::LeftBrace => {
                let body = self.block()?;
                Ok(Stmt {
                    kind: StmtKind::Block(body),
                    line,
                })
            }
            TokenKind::Identifier(name) if self.next_is(&TokenKind::Dollar) => {
                if !self.macros.contains_key(&name) {
                    return Err(Error::new(
                        line,
                        format!(
                            "unknown macro '{name}' (define it with 'macro {name}(...)' first)"
                        ),
                    ));
                }

                self.advance(); // the macro name
                self.advance(); // '$'
                self.expand_macro(&name, line)
            }
            _ => self.expression_statement(line),
        }
    }

    fn import_statement(&mut self, line: usize) -> Result<Stmt, Error> {
        let mut imports = Vec::new();

        loop {
            let token = self.peek().clone();
            let raw = match token.kind {
                TokenKind::Text(path) => {
                    self.advance();
                    path
                }
                TokenKind::Identifier(name) => {
                    self.advance();
                    let mut path = name;

                    // Unquoted module paths may contain `/`: `import lib/math`.
                    // Anything more exotic should use a quoted path.
                    while self.match_kind(&TokenKind::Slash) {
                        let segment = self.consume_identifier(
                            "expected a module name after '/' in the import path",
                        )?;
                        path.push('/');
                        path.push_str(&segment);
                    }

                    path
                }
                other => {
                    return Err(Error::new(
                        token.line,
                        format!(
                            "expected a file path or module name after 'import', found {}",
                            describe(&other)
                        ),
                    ));
                }
            };

            imports.push(self.load_import(&raw, token.line)?);

            if !self.match_kind(&TokenKind::Comma) {
                break;
            }
            self.skip_newlines();
        }

        if imports.len() == 1 {
            Ok(imports.remove(0))
        } else {
            Ok(Stmt {
                kind: StmtKind::Block(imports),
                line,
            })
        }
    }

    fn load_import(&mut self, raw: &str, line: usize) -> Result<Stmt, Error> {
        let canonical = self.resolve_import(raw, line)?;

        if self.loading.contains(&canonical) {
            return Err(Error::new(
                line,
                format!("cyclic import of '{}'", canonical.display()),
            ));
        }

        let display = canonical.display().to_string();
        let source = fs::read_to_string(&canonical).map_err(|error| {
            Error::new(
                line,
                format!("cannot import '{}': {error}", canonical.display()),
            )
        })?;
        let tokens = lex(&source).map_err(|error| error.with_file(display.clone()))?;
        let base_dir = canonical
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();

        // Parse the imported file with the same macro table, then restore the
        // importer's tokens and file context. This lets macros defined in an
        // imported file be used by the file that imports it.
        let saved_tokens = std::mem::replace(&mut self.tokens, tokens);
        let saved_current = std::mem::replace(&mut self.current, 0);
        let saved_file = self.current_file.replace(canonical.clone());
        let saved_base = std::mem::replace(&mut self.base_dir, base_dir);
        self.loading.insert(canonical.clone());

        let result = self.program();

        self.loading.remove(&canonical);
        self.tokens = saved_tokens;
        self.current = saved_current;
        self.current_file = saved_file;
        self.base_dir = saved_base;

        let statements = result.map_err(|error| error.with_file(display.clone()))?;

        Ok(Stmt {
            kind: StmtKind::SourceFile {
                file: display,
                key: canonical.display().to_string(),
                statements,
            },
            line,
        })
    }

    fn resolve_import(&self, raw: &str, line: usize) -> Result<PathBuf, Error> {
        let path = Path::new(raw);
        let path = if path.extension().is_none() {
            path.with_extension("dsq")
        } else {
            path.to_path_buf()
        };

        let explicit_relative = raw.starts_with("./")
            || raw.starts_with("../")
            || raw.starts_with(".\\")
            || raw.starts_with("..\\");

        let candidates = if path.is_absolute() {
            vec![path]
        } else if explicit_relative {
            vec![self.base_dir.join(path)]
        } else {
            let mut candidates = Vec::new();

            if let Some(install_dir) = &self.install_dir {
                candidates.push(install_dir.join(&path));
                candidates.push(install_dir.join("lib").join(&path));
            }

            candidates.push(self.base_dir.join(&path));
            candidates
        };

        for candidate in &candidates {
            if let Ok(canonical) = fs::canonicalize(candidate) {
                return Ok(canonical);
            }
        }

        let tried = candidates
            .iter()
            .map(|candidate| format!("'{}'", candidate.display()))
            .collect::<Vec<_>>()
            .join(", ");

        Err(Error::new(
            line,
            format!("cannot import '{raw}': tried {tried}"),
        ))
    }

    fn function_statement(&mut self, line: usize) -> Result<Stmt, Error> {
        let name = self.consume_identifier("expected a function name")?;
        let type_params = self.type_parameters()?;

        self.consume(
            &TokenKind::LeftParen,
            "expected '(' after the function name",
        )?;
        let params = self.parameters()?;
        self.check_type_parameters(&type_params, &params, line)?;
        let body = with_implicit_return(self.block()?);

        Ok(Stmt {
            kind: StmtKind::Function {
                name,
                type_params,
                params,
                body,
            },
            line,
        })
    }

    fn type_parameters(&mut self) -> Result<Vec<TypeParameter>, Error> {
        let mut type_params: Vec<TypeParameter> = Vec::new();

        if !self.match_kind(&TokenKind::Less) {
            return Ok(type_params);
        }

        self.skip_newlines();

        if self.check(&TokenKind::Greater) {
            return Err(Error::new(
                self.peek().line,
                "type parameter list cannot be empty",
            ));
        }

        loop {
            let param_line = self.peek().line;
            let name = self.consume_identifier("expected a type parameter name")?;

            if type_params.iter().any(|param| param.name == name) {
                return Err(Error::new(
                    param_line,
                    format!("duplicate type parameter '{name}'"),
                ));
            }

            let bound = if self.match_kind(&TokenKind::Colon) {
                Some(self.consume_identifier("expected a trait name after ':'")?)
            } else {
                None
            };

            type_params.push(TypeParameter { name, bound });
            self.skip_newlines();

            if self.match_kind(&TokenKind::Comma) {
                self.skip_newlines();
                if self.check(&TokenKind::Greater) {
                    break;
                }
                continue;
            }
            break;
        }

        self.consume(
            &TokenKind::Greater,
            "expected '>' to close the type parameter list",
        )?;
        Ok(type_params)
    }

    fn check_type_parameters(
        &self,
        type_params: &[TypeParameter],
        params: &[Parameter],
        line: usize,
    ) -> Result<(), Error> {
        if let Some(extra) = type_params.get(params.len()) {
            return Err(Error::new(
                line,
                format!(
                    "type parameter '{}' has no matching function parameter",
                    extra.name
                ),
            ));
        }

        for (type_param, param) in type_params.iter().zip(params) {
            if type_param.name == param.name {
                return Err(Error::new(
                    line,
                    format!(
                        "parameter '{}' conflicts with the type parameter of the same name",
                        param.name
                    ),
                ));
            }
        }

        Ok(())
    }

    fn type_annotation(&mut self) -> Result<String, Error> {
        let name = self.consume_identifier("expected a type name after ':'")?;

        if !self.match_kind(&TokenKind::LeftBracket) {
            return Ok(name);
        }

        self.skip_newlines();
        let mut arguments: Vec<String> = Vec::new();
        loop {
            arguments.push(self.type_annotation()?);
            self.skip_newlines();
            if self.match_kind(&TokenKind::Comma) {
                self.skip_newlines();
                continue;
            }
            break;
        }

        self.consume(
            &TokenKind::RightBracket,
            "expected ']' after type arguments",
        )?;
        Ok(format!("{name}[{}]", arguments.join(", ")))
    }

    fn parameters(&mut self) -> Result<Vec<Parameter>, Error> {
        let mut params: Vec<Parameter> = Vec::new();
        let mut seen_default = false;

        self.skip_newlines();
        if self.match_kind(&TokenKind::RightParen) {
            return Ok(params);
        }

        loop {
            let param_line = self.peek().line;
            let name = self.consume_identifier("expected a parameter name")?;

            let type_annotation = if self.match_kind(&TokenKind::Colon) {
                Some(self.type_annotation()?)
            } else {
                None
            };

            let default = if self.match_kind(&TokenKind::Assign) {
                Some(self.expression()?)
            } else {
                None
            };

            if default.is_none() && seen_default {
                return Err(Error::new(
                    param_line,
                    format!(
                        "parameter '{name}' without a default cannot follow one with a default"
                    ),
                ));
            }
            seen_default |= default.is_some();

            params.push(Parameter {
                name,
                type_annotation,
                default,
            });

            self.skip_newlines();
            if self.match_kind(&TokenKind::Comma) {
                self.skip_newlines();
                continue;
            }
            break;
        }

        self.consume(
            &TokenKind::RightParen,
            "expected ')' to close the parameter list",
        )?;
        Ok(params)
    }

    fn struct_statement(&mut self, line: usize) -> Result<Stmt, Error> {
        let name = self.consume_identifier("expected a struct name after 'struct'")?;
        self.consume(
            &TokenKind::LeftBrace,
            "expected '{' after the struct name",
        )?;
        self.skip_separators();

        let mut fields: Vec<StructField> = Vec::new();
        let mut seen_default = false;
        while !self.check(&TokenKind::RightBrace) && !self.check(&TokenKind::Eof) {
            let field_line = self.peek().line;
            let field_name = self.consume_identifier("expected a field name")?;

            if fields.iter().any(|field| field.name == field_name) {
                return Err(Error::new(
                    field_line,
                    format!("struct '{name}' has duplicate field '{field_name}'"),
                ));
            }

            let default = if self.match_kind(&TokenKind::Assign) {
                Some(self.expression()?)
            } else {
                None
            };

            if default.is_none() && seen_default {
                return Err(Error::new(
                    field_line,
                    format!(
                        "field '{field_name}' without a default cannot follow one with a default"
                    ),
                ));
            }
            seen_default |= default.is_some();

            fields.push(StructField {
                name: field_name,
                default,
            });

            self.skip_newlines();
            self.match_kind(&TokenKind::Comma);
            self.skip_separators();
        }

        self.consume(&TokenKind::RightBrace, "expected '}' to close the struct")?;
        Ok(Stmt {
            kind: StmtKind::Struct { name, fields },
            line,
        })
    }

    fn impl_statement(&mut self, line: usize) -> Result<Stmt, Error> {
        let first = self.consume_identifier("expected a struct or trait name after 'impl'")?;

        let (trait_name, name) = if self.match_kind(&TokenKind::For) {
            let struct_name = self.consume_identifier("expected a struct name after 'for'")?;
            (Some(first), struct_name)
        } else {
            (None, first)
        };

        self.consume(&TokenKind::LeftBrace, "expected '{' after the impl name")?;
        self.skip_separators();

        let mut methods: Vec<Method> = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.check(&TokenKind::Eof) {
            let method_line = self.peek().line;
            self.consume(&TokenKind::Def, "expected 'def' for a method")?;
            let method_name = self.consume_identifier("expected a method name")?;

            if methods.iter().any(|method| method.name == method_name) {
                return Err(Error::new(
                    method_line,
                    format!("duplicate method '{method_name}' in impl for '{name}'"),
                ));
            }

            self.consume(
                &TokenKind::LeftParen,
                "expected '(' after the method name",
            )?;
            let params = self.parameters()?;

            if params.first().map(|param| param.name.as_str()) != Some("self") {
                return Err(Error::new(
                    method_line,
                    format!("method '{name}.{method_name}' must start with 'self'"),
                ));
            }

            let body = with_implicit_return(self.block()?);
            methods.push(Method {
                name: method_name,
                params,
                body,
            });
            self.skip_separators();
        }

        self.consume(&TokenKind::RightBrace, "expected '}' to close the impl")?;
        Ok(Stmt {
            kind: StmtKind::Impl {
                trait_name,
                name,
                methods,
            },
            line,
        })
    }

    fn trait_statement(&mut self, line: usize) -> Result<Stmt, Error> {
        let name = self.consume_identifier("expected a trait name after 'trait'")?;
        self.consume(&TokenKind::LeftBrace, "expected '{' after the trait name")?;
        self.skip_separators();

        let mut methods: Vec<TraitMethod> = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.check(&TokenKind::Eof) {
            let method_line = self.peek().line;
            self.consume(&TokenKind::Def, "expected 'def' for a trait method")?;
            let method_name = self.consume_identifier("expected a method name")?;

            if methods.iter().any(|method| method.name == method_name) {
                return Err(Error::new(
                    method_line,
                    format!("trait '{name}' has duplicate method '{method_name}'"),
                ));
            }

            self.consume(
                &TokenKind::LeftParen,
                "expected '(' after the method name",
            )?;
            let params = self.parameters()?;

            if params.first().map(|param| param.name.as_str()) != Some("self") {
                return Err(Error::new(
                    method_line,
                    format!("trait method '{name}.{method_name}' must start with 'self'"),
                ));
            }

            // A `{` (on this line or the next) is a default body; otherwise the
            // method is required and the implementation must provide it.
            let has_body = self.check(&TokenKind::LeftBrace)
                || self.next_non_newline_is(&TokenKind::LeftBrace);
            let body = if has_body {
                self.skip_newlines();
                Some(with_implicit_return(self.block()?))
            } else {
                None
            };

            if body.is_none() {
                self.end_statement()?;
            }

            methods.push(TraitMethod {
                name: method_name,
                params,
                body,
            });
            self.skip_separators();
        }

        self.consume(&TokenKind::RightBrace, "expected '}' to close the trait")?;
        Ok(Stmt {
            kind: StmtKind::Trait { name, methods },
            line,
        })
    }

    fn macro_definition(&mut self, line: usize) -> Result<Stmt, Error> {
        let name = self.consume_identifier("expected a macro name after 'macro'")?;
        self.consume(&TokenKind::LeftParen, "expected '(' after the macro name")?;
        self.skip_newlines();

        let mut params: Vec<String> = Vec::new();
        if !self.match_kind(&TokenKind::RightParen) {
            loop {
                self.consume(
                    &TokenKind::Dollar,
                    "macro parameters must start with '$'",
                )?;
                let param = self.consume_identifier("expected a parameter name after '$'")?;
                if params.contains(&param) {
                    return Err(Error::new(
                        self.peek().line,
                        format!("duplicate macro parameter '${param}'"),
                    ));
                }
                params.push(param);
                self.skip_newlines();

                if self.match_kind(&TokenKind::Comma) {
                    self.skip_newlines();
                    if self.check(&TokenKind::RightParen) {
                        break;
                    }
                    continue;
                }
                break;
            }

            self.consume(
                &TokenKind::RightParen,
                "expected ')' to close the macro parameters",
            )?;
        }

        self.consume(
            &TokenKind::LeftBrace,
            "expected '{' to start the macro body",
        )?;
        let body = self.capture_balanced(
            TokenKind::LeftBrace,
            TokenKind::RightBrace,
            "macro body",
        )?;

        self.macros.insert(name, MacroDef { params, body });

        // A macro is compile-time only: this statement does nothing at runtime.
        Ok(Stmt {
            kind: StmtKind::Block(Vec::new()),
            line,
        })
    }

    fn expand_macro(&mut self, name: &str, line: usize) -> Result<Stmt, Error> {
        if self.macro_depth >= 32 {
            return Err(Error::new(
                line,
                format!("macro '{name}' expanded too deeply (recursive macro?)"),
            ));
        }

        let definition = self
            .macros
            .get(name)
            .cloned()
            .expect("the caller checked that the macro exists");

        self.consume(&TokenKind::LeftParen, "expected '(' after the macro name")?;
        let arguments = self.capture_macro_arguments()?;

        if arguments.len() != definition.params.len() {
            return Err(Error::new(
                line,
                format!(
                    "macro '{name}' takes {} argument(s), but {} given",
                    definition.params.len(),
                    arguments.len()
                ),
            ));
        }

        let mut expanded: Vec<Token> = Vec::new();
        let mut index = 0;

        while index < definition.body.len() {
            let token = &definition.body[index];

            if token.kind == TokenKind::Dollar {
                let param = match definition.body.get(index + 1) {
                    Some(Token {
                        kind: TokenKind::Identifier(param),
                        ..
                    }) => param,
                    // `name$(` is a nested macro call, not a parameter.
                    _ => {
                        let mut token = token.clone();
                        token.line = line;
                        expanded.push(token);
                        index += 1;
                        continue;
                    }
                };

                let Some(position) = definition
                    .params
                    .iter()
                    .position(|candidate| candidate == param)
                else {
                    return Err(Error::new(
                        token.line,
                        format!("macro '{name}' has no parameter '${param}'"),
                    ));
                };

                expanded.extend(arguments[position].iter().cloned());
                index += 2;
                continue;
            }

            let mut token = token.clone();
            token.line = line;
            expanded.push(token);
            index += 1;
        }

        expanded.push(Token {
            kind: TokenKind::Eof,
            line,
        });

        let mut parser = Parser {
            tokens: expanded,
            current: 0,
            macros: self.macros.clone(),
            macro_depth: self.macro_depth + 1,
            current_file: self.current_file.clone(),
            base_dir: self.base_dir.clone(),
            install_dir: self.install_dir.clone(),
            loading: self.loading.clone(),
        };
        let statements = parser.program()?;

        Ok(Stmt {
            kind: StmtKind::Block(statements),
            line,
        })
    }

    fn capture_balanced(
        &mut self,
        open: TokenKind,
        close: TokenKind,
        what: &str,
    ) -> Result<Vec<Token>, Error> {
        let mut depth = 1usize;
        let mut tokens: Vec<Token> = Vec::new();

        while depth > 0 {
            if self.check(&TokenKind::Eof) {
                return Err(Error::new(
                    self.peek().line,
                    format!("unterminated {what}"),
                ));
            }

            let token = self.advance().clone();

            if std::mem::discriminant(&token.kind) == std::mem::discriminant(&open) {
                depth += 1;
            } else if std::mem::discriminant(&token.kind) == std::mem::discriminant(&close) {
                depth -= 1;
            }

            if depth > 0 {
                tokens.push(token);
            }
        }

        Ok(tokens)
    }

    fn capture_macro_arguments(&mut self) -> Result<Vec<Vec<Token>>, Error> {
        let mut groups: Vec<Vec<Token>> = Vec::new();
        let mut current: Vec<Token> = Vec::new();
        let mut depth = 0usize;

        if self.match_kind(&TokenKind::RightParen) {
            return Ok(groups);
        }

        loop {
            if self.check(&TokenKind::Eof) {
                return Err(Error::new(
                    self.peek().line,
                    "unterminated macro call (missing ')')",
                ));
            }

            let token = self.advance().clone();

            match token.kind {
                TokenKind::LeftParen | TokenKind::LeftBrace | TokenKind::LeftBracket => {
                    depth += 1;
                    current.push(token);
                }
                TokenKind::RightParen if depth == 0 => {
                    trim_newlines(&mut current);
                    if !current.is_empty() || groups.is_empty() {
                        groups.push(current);
                    }
                    return Ok(groups);
                }
                TokenKind::RightParen | TokenKind::RightBrace | TokenKind::RightBracket => {
                    depth = depth.saturating_sub(1);
                    current.push(token);
                }
                TokenKind::Comma if depth == 0 => {
                    let mut group = std::mem::take(&mut current);
                    trim_newlines(&mut group);
                    groups.push(group);
                }
                _ => current.push(token),
            }
        }
    }

    fn return_statement(&mut self, line: usize) -> Result<Stmt, Error> {
        let value = if self.check(&TokenKind::Newline)
            || self.check(&TokenKind::Semicolon)
            || self.check(&TokenKind::RightBrace)
            || self.check(&TokenKind::Eof)
        {
            None
        } else {
            Some(self.expression()?)
        };

        Ok(Stmt {
            kind: StmtKind::Return(value),
            line,
        })
    }

    fn if_statement(&mut self, line: usize) -> Result<Stmt, Error> {
        let condition = self.expression()?;
        let then_branch = self.block()?;

        let mut else_branch = None;

        // `elif`/`else` may start on the same line as `}` or the line after it.
        let saved = self.current;
        self.skip_separators();

        if self.check(&TokenKind::Elif) {
            let elif_line = self.peek().line;
            self.advance();
            else_branch = Some(vec![self.if_statement(elif_line)?]);
        } else if self.check(&TokenKind::Else) {
            self.advance();
            if self.check(&TokenKind::If) {
                // `else if` is accepted as a nested `if`; `elif` is preferred.
                let nested_line = self.peek().line;
                self.advance();
                else_branch = Some(vec![self.if_statement(nested_line)?]);
            } else {
                else_branch = Some(self.block()?);
            }
        } else {
            self.current = saved;
        }

        Ok(Stmt {
            kind: StmtKind::If {
                condition,
                then_branch,
                else_branch,
            },
            line,
        })
    }

    fn expression_statement(&mut self, line: usize) -> Result<Stmt, Error> {
        let expr = self.expression()?;

        if self.match_kind(&TokenKind::Assign) {
            let value = self.expression()?;

            return match expr {
                Expr::Var(name) => Ok(Stmt {
                    kind: StmtKind::Assign { name, value },
                    line,
                }),
                Expr::Index { object, index } => {
                    let Expr::Var(name) = *object else {
                        return Err(Error::new(
                            line,
                            "only a variable lookup or map entry can be assigned to",
                        ));
                    };

                    Ok(Stmt {
                        kind: StmtKind::IndexAssign {
                            name,
                            index: *index,
                            value,
                        },
                        line,
                    })
                }
                Expr::Slice {
                    object,
                    start,
                    stop,
                    step,
                } => {
                    let Expr::Var(name) = *object else {
                        return Err(Error::new(
                            line,
                            "only a variable list or text can be slice-assigned",
                        ));
                    };

                    Ok(Stmt {
                        kind: StmtKind::SliceAssign {
                            name,
                            start: start.map(|expr| *expr),
                            stop: stop.map(|expr| *expr),
                            step: step.map(|expr| *expr),
                            value,
                        },
                        line,
                    })
                }
                Expr::Field { object, name: field } => {
                    let Expr::Var(name) = *object else {
                        return Err(Error::new(
                            line,
                            "only a variable field can be assigned to",
                        ));
                    };

                    Ok(Stmt {
                        kind: StmtKind::FieldAssign { name, field, value },
                        line,
                    })
                }
                _ => Err(Error::new(
                    line,
                    "only a variable, index, slice, or struct field can be assigned to",
                )),
            };
        }

        Ok(Stmt {
            kind: StmtKind::Expression(expr),
            line,
        })
    }

    fn block(&mut self) -> Result<Vec<Stmt>, Error> {
        self.skip_newlines();
        self.consume(&TokenKind::LeftBrace, "expected '{' to start a block")?;
        self.skip_separators();

        let mut statements: Vec<Stmt> = Vec::new();
        while !self.check(&TokenKind::RightBrace) && !self.check(&TokenKind::Eof) {
            statements.push(self.statement()?);
            self.end_statement()?;
            self.skip_separators();
        }

        self.consume(&TokenKind::RightBrace, "expected '}' to close the block")?;
        Ok(statements)
    }

    fn expression(&mut self) -> Result<Expr, Error> {
        self.or_expression()
    }

    fn or_expression(&mut self) -> Result<Expr, Error> {
        let mut left = self.and_expression()?;

        while self.match_kind(&TokenKind::Or) {
            let right = self.and_expression()?;
            left = Expr::Binary {
                op: BinaryOp::Or,
                left: Box::new(left),
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn and_expression(&mut self) -> Result<Expr, Error> {
        let mut left = self.equality_expression()?;

        while self.match_kind(&TokenKind::And) {
            let right = self.equality_expression()?;
            left = Expr::Binary {
                op: BinaryOp::And,
                left: Box::new(left),
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn equality_expression(&mut self) -> Result<Expr, Error> {
        let mut left = self.comparison_expression()?;

        loop {
            let op = if self.match_kind(&TokenKind::Equal) {
                BinaryOp::Equal
            } else if self.match_kind(&TokenKind::NotEqual) {
                BinaryOp::NotEqual
            } else {
                break;
            };

            let right = self.comparison_expression()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn comparison_expression(&mut self) -> Result<Expr, Error> {
        let mut left = self.additive_expression()?;

        loop {
            let op = if self.match_kind(&TokenKind::Greater) {
                BinaryOp::Greater
            } else if self.match_kind(&TokenKind::GreaterEqual) {
                BinaryOp::GreaterEqual
            } else if self.match_kind(&TokenKind::Less) {
                BinaryOp::Less
            } else if self.match_kind(&TokenKind::LessEqual) {
                BinaryOp::LessEqual
            } else if self.match_kind(&TokenKind::In) {
                BinaryOp::In
            } else if self.check(&TokenKind::Not) && self.next_is(&TokenKind::In) {
                self.advance();
                self.advance();
                BinaryOp::NotIn
            } else {
                break;
            };

            let right = self.additive_expression()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn additive_expression(&mut self) -> Result<Expr, Error> {
        let mut left = self.multiplicative_expression()?;

        loop {
            let op = if self.match_kind(&TokenKind::Plus) {
                BinaryOp::Add
            } else if self.match_kind(&TokenKind::Minus) {
                BinaryOp::Subtract
            } else {
                break;
            };

            let right = self.multiplicative_expression()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn multiplicative_expression(&mut self) -> Result<Expr, Error> {
        let mut left = self.unary_expression()?;

        loop {
            let op = if self.match_kind(&TokenKind::Star) {
                BinaryOp::Multiply
            } else if self.match_kind(&TokenKind::Slash) {
                BinaryOp::Divide
            } else if self.match_kind(&TokenKind::Percent) {
                BinaryOp::Remainder
            } else {
                break;
            };

            let right = self.unary_expression()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn unary_expression(&mut self) -> Result<Expr, Error> {
        if self.match_kind(&TokenKind::Not) {
            let expr = self.unary_expression()?;
            return Ok(Expr::Unary {
                op: UnaryOp::Not,
                expr: Box::new(expr),
            });
        }

        if self.match_kind(&TokenKind::Minus) {
            let expr = self.unary_expression()?;
            return Ok(Expr::Unary {
                op: UnaryOp::Neg,
                expr: Box::new(expr),
            });
        }

        self.postfix_expression()
    }

    fn postfix_expression(&mut self) -> Result<Expr, Error> {
        let mut expr = self.primary_expression()?;

        loop {
            if self.match_kind(&TokenKind::LeftBracket) {
                self.skip_newlines();

                let start =
                    if self.check(&TokenKind::Colon) || self.check(&TokenKind::RightBracket) {
                        None
                    } else {
                        Some(Box::new(self.expression()?))
                    };

                self.skip_newlines();

                if self.match_kind(&TokenKind::Colon) {
                    let stop =
                        if self.check(&TokenKind::Colon) || self.check(&TokenKind::RightBracket) {
                            None
                        } else {
                            Some(Box::new(self.expression()?))
                        };

                    self.skip_newlines();
                    let step = if self.match_kind(&TokenKind::Colon) {
                        self.skip_newlines();
                        if self.check(&TokenKind::RightBracket) {
                            None
                        } else {
                            Some(Box::new(self.expression()?))
                        }
                    } else {
                        None
                    };

                    self.skip_newlines();
                    self.consume(&TokenKind::RightBracket, "expected ']' after the slice")?;
                    expr = Expr::Slice {
                        object: Box::new(expr),
                        start,
                        stop,
                        step,
                    };
                } else {
                    let Some(index) = start else {
                        return Err(Error::new(
                            self.peek().line,
                            "expected an index or slice inside '[]'",
                        ));
                    };

                    self.consume(&TokenKind::RightBracket, "expected ']' after the index")?;
                    expr = Expr::Index {
                        object: Box::new(expr),
                        index,
                    };
                }

                continue;
            }

            if self.match_kind(&TokenKind::Dot) {
                let name = self.consume_identifier("expected a field or method name after '.'")?;

                if self.match_kind(&TokenKind::LeftParen) {
                    let args = self.arguments()?;
                    expr = Expr::Call {
                        callee: Callee::Method {
                            receiver: Box::new(expr),
                            method: name,
                        },
                        args,
                    };
                } else {
                    expr = Expr::Field {
                        object: Box::new(expr),
                        name,
                    };
                }
                continue;
            }

            break;
        }

        Ok(expr)
    }

    fn primary_expression(&mut self) -> Result<Expr, Error> {
        let line = self.peek().line;

        match self.peek().kind.clone() {
            TokenKind::Int(number) => {
                self.advance();
                Ok(Expr::Int(number))
            }
            TokenKind::Float(number) => {
                self.advance();
                Ok(Expr::Float(number))
            }
            TokenKind::True => {
                self.advance();
                Ok(Expr::Bool(true))
            }
            TokenKind::False => {
                self.advance();
                Ok(Expr::Bool(false))
            }
            TokenKind::None => {
                self.advance();
                Ok(Expr::None)
            }
            TokenKind::Text(text) => {
                self.advance();
                Ok(Expr::Text(text))
            }
            TokenKind::Identifier(name) => {
                if self.next_is(&TokenKind::Dollar) {
                    return Err(Error::new(
                        line,
                        format!(
                            "macro '{name}' can only be used as a statement: put '{name}$(...)' on its own line"
                        ),
                    ));
                }

                self.advance();

                if name.eq_ignore_ascii_case("list") && self.check(&TokenKind::LeftBracket) {
                    self.advance();
                    let element_type = self.type_annotation()?;
                    self.skip_newlines();
                    self.consume(
                        &TokenKind::RightBracket,
                        "expected ']' after the list element type",
                    )?;
                    self.skip_newlines();
                    self.consume(
                        &TokenKind::LeftParen,
                        "expected '(' after List[...] to list its values",
                    )?;
                    let items = self.arguments()?;
                    return Ok(Expr::TypedList {
                        element_type,
                        items,
                    });
                }

                if self.check(&TokenKind::Not) && self.next_is(&TokenKind::LeftParen) {
                    if !matches!(name.as_str(), "print" | "println" | "eprint" | "eprintln") {
                        return Err(Error::new(
                            line,
                            format!(
                                "unknown format macro '{name}!' (use print!, println!, eprint!, or eprintln!)"
                            ),
                        ));
                    }

                    self.advance();
                    self.consume(
                        &TokenKind::LeftParen,
                        "expected '(' after the format macro name",
                    )?;
                    let args = self.arguments()?;
                    return Ok(Expr::FormatCall { name, args });
                }

                if self.match_kind(&TokenKind::LeftParen) {
                    let args = self.arguments()?;
                    return Ok(Expr::Call {
                        callee: Callee::Function(name),
                        args,
                    });
                }

                Ok(Expr::Var(name))
            }
            TokenKind::LeftBrace => {
                self.advance();
                self.map_literal()
            }
            TokenKind::LeftBracket => {
                self.advance();
                self.list_literal()
            }
            TokenKind::Matrix => {
                self.advance();
                self.matrix_literal()
            }
            TokenKind::LeftParen => {
                self.advance();
                let expr = self.expression()?;
                self.consume(&TokenKind::RightParen, "expected ')'")?;
                Ok(expr)
            }
            other => Err(Error::new(
                line,
                format!("expected a value, found {}", describe(&other)),
            )),
        }
    }

    fn arguments(&mut self) -> Result<Vec<Expr>, Error> {
        let mut args: Vec<Expr> = Vec::new();

        self.skip_newlines();
        if self.match_kind(&TokenKind::RightParen) {
            return Ok(args);
        }

        loop {
            args.push(self.expression()?);
            self.skip_newlines();

            if self.match_kind(&TokenKind::Comma) {
                self.skip_newlines();
                continue;
            }
            break;
        }

        self.consume(
            &TokenKind::RightParen,
            "expected ')' to close the argument list",
        )?;
        Ok(args)
    }

    /// `[1, 2, 3]`, `[]`, and multi-line literals.
    fn list_literal(&mut self) -> Result<Expr, Error> {
        self.skip_newlines();

        let mut items: Vec<Expr> = Vec::new();
        if self.match_kind(&TokenKind::RightBracket) {
            return Ok(Expr::List(items));
        }

        loop {
            items.push(self.expression()?);
            self.skip_newlines();

            if self.match_kind(&TokenKind::Comma) {
                self.skip_newlines();
                if self.match_kind(&TokenKind::RightBracket) {
                    break;
                }
                continue;
            }

            break;
        }

        self.consume(&TokenKind::RightBracket, "expected ']' to close the list")?;
        Ok(Expr::List(items))
    }

    fn map_literal(&mut self) -> Result<Expr, Error> {
        self.skip_separators();

        let mut entries: Vec<(Expr, Expr)> = Vec::new();

        while !self.check(&TokenKind::RightBrace) && !self.check(&TokenKind::Eof) {
            let key = self.expression()?;
            self.consume(
                &TokenKind::Colon,
                "expected ':' between a hashmap key and value",
            )?;
            let value = self.expression()?;
            entries.push((key, value));

            self.skip_newlines();
            self.match_kind(&TokenKind::Comma);
            self.skip_separators();
        }

        self.consume(&TokenKind::RightBrace, "expected '}' to close the hashmap")?;
        Ok(Expr::Map(entries))
    }

    /// `mat{row1: {a: 1, b: 2}, row2: {a: 3}}`
    fn matrix_literal(&mut self) -> Result<Expr, Error> {
        self.consume(&TokenKind::LeftBrace, "expected '{' after 'mat'")?;
        self.skip_separators();

        let mut rows: Vec<(String, Vec<(String, Expr)>)> = Vec::new();

        while !self.check(&TokenKind::RightBrace) && !self.check(&TokenKind::Eof) {
            let name = self.matrix_key("expected a row name")?;
            self.consume(&TokenKind::Colon, "expected ':' after the row name")?;
            let cells = self.matrix_cells()?;
            rows.push((name, cells));

            self.skip_newlines();
            self.match_kind(&TokenKind::Comma);
            self.skip_separators();
        }

        self.consume(&TokenKind::RightBrace, "expected '}' to close the matrix")?;
        Ok(Expr::Matrix(rows))
    }

    fn matrix_cells(&mut self) -> Result<Vec<(String, Expr)>, Error> {
        self.skip_newlines();
        self.consume(
            &TokenKind::LeftBrace,
            "expected '{' to start a matrix row",
        )?;
        self.skip_separators();

        let mut cells: Vec<(String, Expr)> = Vec::new();

        while !self.check(&TokenKind::RightBrace) && !self.check(&TokenKind::Eof) {
            let key = self.matrix_key("expected a cell name")?;
            self.consume(&TokenKind::Colon, "expected ':' after the cell name")?;
            let value = self.expression()?;
            cells.push((key, value));

            self.skip_newlines();
            self.match_kind(&TokenKind::Comma);
            self.skip_separators();
        }

        self.consume(&TokenKind::RightBrace, "expected '}' to close the matrix row")?;
        Ok(cells)
    }

    fn matrix_key(&mut self, message: &str) -> Result<String, Error> {
        match self.peek().kind.clone() {
            TokenKind::Identifier(name) => {
                self.advance();
                Ok(name)
            }
            TokenKind::Text(text) => {
                self.advance();
                Ok(text)
            }
            other => Err(Error::new(
                self.peek().line,
                format!("{message}, found {}", describe(&other)),
            )),
        }
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.current]
    }

    fn advance(&mut self) -> &Token {
        if self.current < self.tokens.len() - 1 {
            self.current += 1;
        }
        &self.tokens[self.current - 1]
    }

    fn check(&self, kind: &TokenKind) -> bool {
        std::mem::discriminant(&self.peek().kind) == std::mem::discriminant(kind)
    }

    fn match_kind(&mut self, kind: &TokenKind) -> bool {
        if self.check(kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn next_is(&self, kind: &TokenKind) -> bool {
        self.tokens
            .get(self.current + 1)
            .is_some_and(|token| std::mem::discriminant(&token.kind) == std::mem::discriminant(kind))
    }

    fn next_non_newline_is(&self, kind: &TokenKind) -> bool {
        self.tokens
            .iter()
            .skip(self.current)
            .find(|token| !matches!(token.kind, TokenKind::Newline))
            .is_some_and(|token| std::mem::discriminant(&token.kind) == std::mem::discriminant(kind))
    }

    fn consume(&mut self, kind: &TokenKind, message: &str) -> Result<Token, Error> {
        if self.check(kind) {
            return Ok(self.advance().clone());
        }
        Err(Error::new(self.peek().line, message))
    }

    fn consume_identifier(&mut self, message: &str) -> Result<String, Error> {
        match self.peek().kind.clone() {
            TokenKind::Identifier(name) => {
                self.advance();
                Ok(name)
            }
            other => Err(Error::new(
                self.peek().line,
                format!("{message}, found {}", describe(&other)),
            )),
        }
    }

    fn end_statement(&mut self) -> Result<(), Error> {
        if self.check(&TokenKind::Semicolon)
            || self.check(&TokenKind::Newline)
            || self.check(&TokenKind::RightBrace)
            || self.check(&TokenKind::Eof)
        {
            return Ok(());
        }

        Err(Error::new(
            self.peek().line,
            format!(
                "unexpected {} after a statement (missing a newline or ';'?)",
                describe(&self.peek().kind)
            ),
        ))
    }

    fn skip_separators(&mut self) {
        while self.check(&TokenKind::Semicolon) || self.check(&TokenKind::Newline) {
            self.advance();
        }
    }

    fn skip_newlines(&mut self) {
        while self.check(&TokenKind::Newline) {
            self.advance();
        }
    }
}

fn installation_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|executable| executable.parent().map(Path::to_path_buf))
}

fn trim_newlines(tokens: &mut Vec<Token>) {
    while matches!(
        tokens.first(),
        Some(Token {
            kind: TokenKind::Newline,
            ..
        })
    ) {
        tokens.remove(0);
    }

    while matches!(
        tokens.last(),
        Some(Token {
            kind: TokenKind::Newline,
            ..
        })
    ) {
        tokens.pop();
    }
}

/// Make the final bare expression in a function or method body its return
/// value, so `def double(x) { x * 2 }` works without an explicit `return`.
fn with_implicit_return(mut statements: Vec<Stmt>) -> Vec<Stmt> {
    let Some(last) = statements.last_mut() else {
        return statements;
    };

    if let StmtKind::Expression(expr) = &mut last.kind {
        let value = std::mem::replace(expr, Expr::None);
        last.kind = StmtKind::Return(Some(value));
    }

    statements
}

fn describe(kind: &TokenKind) -> String {
    match kind {
        TokenKind::Identifier(name) => format!("'{name}'"),
        TokenKind::Int(number) => format!("'{number}'"),
        TokenKind::Float(number) => format!("'{number}'"),
        TokenKind::Text(text) => format!("'{text}'"),
        TokenKind::Eof => "end of input".to_string(),
        TokenKind::Newline => "end of line".to_string(),
        other => format!("'{other:?}'"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expression_of(source: &str) -> Expr {
        let statements = Parser::parse(source).unwrap();
        match statements.into_iter().next().unwrap().kind {
            StmtKind::Expression(expr) => expr,
            StmtKind::Assign { value, .. } => value,
            other => panic!("expected an expression, got {other:?}"),
        }
    }

    #[test]
    fn multiplication_binds_tighter_than_addition() {
        let expr = expression_of("1 + 2 * 3");

        let Expr::Binary { op, right, .. } = expr else {
            panic!("expected a binary expression");
        };
        assert_eq!(op, BinaryOp::Add);
        assert!(matches!(
            *right,
            Expr::Binary {
                op: BinaryOp::Multiply,
                ..
            }
        ));
    }

    #[test]
    fn and_binds_tighter_than_or() {
        let expr = expression_of("a or b and c");

        let Expr::Binary { op, right, .. } = expr else {
            panic!("expected a binary expression");
        };
        assert_eq!(op, BinaryOp::Or);
        assert!(matches!(
            *right,
            Expr::Binary {
                op: BinaryOp::And,
                ..
            }
        ));
    }

    #[test]
    fn comparisons_chain_below_equality() {
        let expr = expression_of("a == b > 1");

        let Expr::Binary { op, right, .. } = expr else {
            panic!("expected a binary expression");
        };
        assert_eq!(op, BinaryOp::Equal);
        assert!(matches!(
            *right,
            Expr::Binary {
                op: BinaryOp::Greater,
                ..
            }
        ));
    }

    #[test]
    fn parses_pythonic_control_flow() {
        let source = "if a > 1 {\n    while a < 10 {\n        a = a + 1\n    }\n} elif a == 1 {\n    print(a)\n} else {\n    for x in hashSet(1, 2, 3) {\n        console.println(x)\n        break\n    }\n}";
        let statements = Parser::parse(source).unwrap();
        assert_eq!(statements.len(), 1);

        let StmtKind::If { else_branch, .. } = &statements[0].kind else {
            panic!("expected an if statement");
        };
        let else_branch = else_branch.as_ref().expect("expected an else branch");
        assert!(matches!(
            else_branch.first().map(|stmt| &stmt.kind),
            Some(StmtKind::If { .. })
        ));
    }

    #[test]
    fn assignment_needs_a_variable() {
        let error = Parser::parse("1 = 2").unwrap_err();
        assert!(error.message.contains("only a variable"), "{error}");
    }

    #[test]
    fn statements_need_a_separator() {
        let error = Parser::parse("x = 1 y = 2").unwrap_err();
        assert!(error.message.contains("after a statement"), "{error}");
    }

    #[test]
    fn parses_hashmaps_and_index_expressions() {
        let expr = expression_of("x = {\"a\": 1, b: 2, 3: \"three\"}");
        let Expr::Map(entries) = expr else {
            panic!("expected a hashmap");
        };
        assert_eq!(entries.len(), 3);

        let expr = expression_of(r#"data["a"][0]"#);
        assert!(matches!(expr, Expr::Index { .. }));
    }

    #[test]
    fn parses_index_assignment() {
        let statements = Parser::parse("d[\"a\"] = 1").unwrap();
        assert!(matches!(
            &statements[0].kind,
            StmtKind::IndexAssign { .. }
        ));
    }

    #[test]
    fn parses_braced_matrix_literal() {
        let expr = expression_of("mat{row1: {a: 1, b: 2}, row2: {a: 3}}");
        let Expr::Matrix(rows) = expr else {
            panic!("expected a matrix");
        };

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0, "row1");
        assert_eq!(rows[0].1.len(), 2);
        assert_eq!(rows[1].0, "row2");
    }

    #[test]
    fn parses_none_and_definitions() {
        let statements =
            Parser::parse("def f(a, b = None) {\n    return None\n}\nx = f(1)").unwrap();
        assert_eq!(statements.len(), 2);

        let StmtKind::Function { params, .. } = &statements[0].kind else {
            panic!("expected a function");
        };
        assert_eq!(params.len(), 2);
        assert_eq!(params[1].default, Some(Expr::None));
    }

    #[test]
    fn parses_function_trait_bounds() {
        let statements = Parser::parse(
            "def show<T: Shape, U>(value, other, third = 1) {\n    return value\n}",
        )
        .unwrap();

        let StmtKind::Function {
            type_params,
            params,
            ..
        } = &statements[0].kind
        else {
            panic!("expected a function");
        };
        assert_eq!(type_params.len(), 2);
        assert_eq!(type_params[0].name, "T");
        assert_eq!(type_params[0].bound.as_deref(), Some("Shape"));
        assert_eq!(type_params[1].name, "U");
        assert!(type_params[1].bound.is_none());
        assert_eq!(params.len(), 3);
    }

    #[test]
    fn function_type_parameters_are_checked() {
        let error = Parser::parse("def f<T: Shape, U: Shape>(x) { return x }").unwrap_err();
        assert!(
            error
                .message
                .contains("type parameter 'U' has no matching function parameter"),
            "{error}"
        );

        let error = Parser::parse("def f<T, T>(x) { return x }").unwrap_err();
        assert!(error.message.contains("duplicate type parameter 'T'"), "{error}");

        let error = Parser::parse("def f<T>(T) { return T }").unwrap_err();
        assert!(
            error.message.contains("conflicts with the type parameter"),
            "{error}"
        );

        let error = Parser::parse("def f<>(x) { return x }").unwrap_err();
        assert!(error.message.contains("cannot be empty"), "{error}");
    }

    #[test]
    fn parses_structs_and_impl_methods() {
        let source = "struct Point {\n    x = 0\n    y = 0\n}\n\
                      impl Point {\n    def sum(self) {\n        return self.x + self.y\n    }\n}\n\
                      p = Point(1, 2)\n\
                      p.x = 3";
        let statements = Parser::parse(source).unwrap();

        let StmtKind::Struct { name, fields } = &statements[0].kind else {
            panic!("expected a struct");
        };
        assert_eq!(name, "Point");
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].name, "x");
        assert_eq!(fields[0].default, Some(Expr::Int(0)));

        let StmtKind::Impl {
            trait_name,
            name,
            methods,
        } = &statements[1].kind
        else {
            panic!("expected an impl");
        };
        assert!(trait_name.is_none());
        assert_eq!(name, "Point");
        assert_eq!(methods.len(), 1);
        assert_eq!(methods[0].name, "sum");
        assert_eq!(methods[0].params[0].name, "self");

        assert!(matches!(
            &statements[3].kind,
            StmtKind::FieldAssign { field, .. } if field == "x"
        ));
    }

    #[test]
    fn impl_methods_must_take_self() {
        let error = Parser::parse("struct P { x }\nimpl P { def m(a) { return a } }")
            .unwrap_err();
        assert!(error.message.contains("must start with 'self'"), "{error}");
    }

    #[test]
    fn traits_have_required_and_default_methods() {
        let statements = Parser::parse(
            "trait Shape {\n\
             def area(self)\n\
             def describe(self) {\n\
             return self.area()\n\
             }\n\
             }\n\
             struct Circle { radius }\n\
             impl Shape for Circle {\n\
             def area(self) {\n\
             return self.radius\n\
             }\n\
             }",
        )
        .unwrap();

        let StmtKind::Trait { name, methods } = &statements[0].kind else {
            panic!("expected a trait");
        };
        assert_eq!(name, "Shape");
        assert_eq!(methods.len(), 2);
        assert_eq!(methods[0].name, "area");
        assert_eq!(methods[0].params[0].name, "self");
        assert!(methods[0].body.is_none());
        assert_eq!(methods[1].name, "describe");
        assert!(methods[1].body.is_some());

        let StmtKind::Impl {
            trait_name,
            name,
            methods,
        } = &statements[2].kind
        else {
            panic!("expected a trait impl");
        };
        assert_eq!(trait_name.as_deref(), Some("Shape"));
        assert_eq!(name, "Circle");
        assert_eq!(methods[0].name, "area");
    }

    #[test]
    fn trait_methods_must_take_self() {
        let error = Parser::parse("trait T { def m(a) }").unwrap_err();
        assert!(error.message.contains("must start with 'self'"), "{error}");
    }

    #[test]
    fn duplicate_trait_methods_are_errors() {
        let error = Parser::parse("trait T {\n    def m(self)\n    def m(self)\n}").unwrap_err();
        assert!(
            error.message.contains("duplicate method 'm'"),
            "{error}"
        );
    }

    #[test]
    fn duplicate_impl_methods_are_errors() {
        let error = Parser::parse(
            "struct S { x }\nimpl S {\n    def m(self) { }\n    def m(self) { }\n}",
        )
        .unwrap_err();
        assert!(
            error.message.contains("duplicate method 'm' in impl for 'S'"),
            "{error}"
        );
    }

    #[test]
    fn expands_statement_macros() {
        let statements = Parser::parse(
            "macro twice($body,) {\n    $body\n    $body\n}\ntwice$({ print(1) },)",
        )
        .unwrap();

        assert_eq!(statements.len(), 2);
        let StmtKind::Block(expanded) = &statements[1].kind else {
            panic!("expected expanded macro statements");
        };
        assert_eq!(expanded.len(), 2);
    }

    #[test]
    fn macro_argument_count_is_checked() {
        let error = Parser::parse("macro m($a) { $a }\nm$()").unwrap_err();
        assert!(
            error.message.contains("takes 1 argument(s), but 0 given"),
            "{error}"
        );
    }

    #[test]
    fn unknown_macro_parameters_are_errors() {
        let error = Parser::parse("macro m($a) { $b }\nm$(1)").unwrap_err();
        assert!(error.message.contains("no parameter '$b'"), "{error}");
    }

    #[test]
    fn recursive_macros_stop() {
        let error = Parser::parse("macro loopy($x) { loopy$($x) }\nloopy$(1)").unwrap_err();
        assert!(error.message.contains("expanded too deeply"), "{error}");
    }

    #[test]
    fn macro_calls_need_the_dollar_suffix() {
        let error = Parser::parse("nope$(1)").unwrap_err();
        assert!(error.message.contains("unknown macro 'nope'"), "{error}");

        let error = Parser::parse("macro m($a) { $a }\nx = m$(1)").unwrap_err();
        assert!(
            error.message.contains("can only be used as a statement"),
            "{error}"
        );
    }

    fn import_test_root(name: &str) -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};

        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "dsh2_parser_import_{}_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
            name
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn imported_file(statements: &[Stmt]) -> PathBuf {
        let StmtKind::SourceFile { key, .. } = &statements[0].kind else {
            panic!("expected an imported file");
        };
        PathBuf::from(key)
    }

    #[test]
    fn imports_search_the_install_dir_before_the_script_dir() {
        let root = import_test_root("install_first");
        let install = root.join("install");
        let project = root.join("project");
        fs::create_dir_all(&install).unwrap();
        fs::create_dir_all(&project).unwrap();
        fs::write(install.join("lib.dsq"), "value = 1").unwrap();
        fs::write(project.join("lib.dsq"), "value = 2").unwrap();
        let main = project.join("main.dsq");
        fs::write(&main, "import lib").unwrap();

        let statements =
            Parser::parse_file_with_install_dir(&main, Some(install.clone())).unwrap();

        assert_eq!(
            imported_file(&statements),
            fs::canonicalize(install.join("lib.dsq")).unwrap()
        );
    }

    #[test]
    fn imports_fall_back_to_the_script_dir() {
        let root = import_test_root("script_fallback");
        let install = root.join("install");
        let project = root.join("project");
        fs::create_dir_all(&install).unwrap();
        fs::create_dir_all(&project).unwrap();
        fs::write(project.join("lib.dsq"), "value = 2").unwrap();
        let main = project.join("main.dsq");
        fs::write(&main, "import lib").unwrap();

        let statements =
            Parser::parse_file_with_install_dir(&main, Some(install)).unwrap();

        assert_eq!(
            imported_file(&statements),
            fs::canonicalize(project.join("lib.dsq")).unwrap()
        );
    }

    #[test]
    fn imports_search_the_install_lib_subdirectory() {
        let root = import_test_root("install_lib");
        let install = root.join("install");
        let project = root.join("project");
        fs::create_dir_all(install.join("lib")).unwrap();
        fs::create_dir_all(&project).unwrap();
        fs::write(install.join("lib").join("helper.dsq"), "value = 1").unwrap();
        let main = project.join("main.dsq");
        fs::write(&main, "import helper").unwrap();

        let statements =
            Parser::parse_file_with_install_dir(&main, Some(install.clone())).unwrap();

        assert_eq!(
            imported_file(&statements),
            fs::canonicalize(install.join("lib").join("helper.dsq")).unwrap()
        );
    }

    #[test]
    fn explicit_relative_imports_skip_the_install_dir() {
        let root = import_test_root("explicit_relative");
        let install = root.join("install");
        let project = root.join("project");
        fs::create_dir_all(&install).unwrap();
        fs::create_dir_all(&project).unwrap();
        fs::write(install.join("lib.dsq"), "value = 1").unwrap();
        fs::write(project.join("lib.dsq"), "value = 2").unwrap();
        let main = project.join("main.dsq");
        fs::write(&main, "import \"./lib\"").unwrap();

        let statements =
            Parser::parse_file_with_install_dir(&main, Some(install)).unwrap();

        assert_eq!(
            imported_file(&statements),
            fs::canonicalize(project.join("lib.dsq")).unwrap()
        );
    }
}
