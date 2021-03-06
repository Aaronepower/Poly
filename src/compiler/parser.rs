use std::collections::HashMap;
use std::iter::Peekable;
use std::vec::IntoIter;

use super::tokens::*;
use super::tokens::AstError::*;
use super::tokens::Lexeme::*;
use super::tokens::Operator::*;
use super::tokens::Token::*;

/// Shortens Result type
pub type AstResult = Result<Token, AstError>;

macro_rules! unexpected_eof {
    ($token:expr) => {
        return Err(UnexpectedEof($token));
    }
}

macro_rules! get_identifer {
    ($token:expr, $index:expr, $unexpected:expr) => {
        match $token {
            Some(Word(_, text)) => text,
            Some(unexpected_token) => {
                return Err($unexpected(unexpected_token))
            }
            None => return Err(UnexpectedEof(Symbol($index, At))),
        };
    }
}

macro_rules! get_namespaced_identifer {
    ($this:expr, $index:expr, $unexpected:expr, $previous:expr) => {
        match $this.take() {
            Some(Word(index, text)) => {
                let mut new_text = text.clone();
                while let Some(Symbol(_, Dot)) = $this.peek() {
                    let _ = $this.take();
                    new_text.push('.');

                    match $this.take() {
                        Some(Word(_, member)) => new_text.push_str(&*member),
                        Some(unexpected_token) => return Err($unexpected(unexpected_token)),
                        None => return Err(UnexpectedEof(Symbol(index, Dot))),
                    }
                }
                new_text
            }
            Some(unexpected_token) => return Err($unexpected(unexpected_token)),
            None => return Err(UnexpectedEof(Symbol($index, $previous))),
        } 
    }
}

macro_rules! get_children {
    ($token:expr, $parent:expr) => 
    {{
        let mut depth: usize = 0;
        let mut open_brace_index: usize = 0;
        let mut close_brace_index: usize = 0;
        let mut children = Vec::new();
        while let Some(token) = $token {
            match token {
                Symbol(index, OpenBrace) => {
                    depth += 1;

                    if depth != 0 {
                        children.push(Symbol(index, OpenBrace));
                    }
                    open_brace_index = index;
                }
                Symbol(index, CloseBrace) => {
                    if depth == 0 {
                        break;
                    } else {
                        depth -= 1;
                        children.push(Symbol(index, CloseBrace));
                    }
                    close_brace_index = index;
                }
                t => children.push(t),
            }
        }

        if depth > 0 {
            return Err(UnclosedOpenBraces(open_brace_index));
        } else if depth != 0 {
            return Err(UnclosedCloseBraces(close_brace_index));
        }
        if !children.is_empty() {
            $parent.add_children(&mut Parser::new(children).output());
        }
    }}
}


/// The struct detailing the parser itself.
pub struct Parser {
    input: Peekable<IntoIter<Lexeme>>,
    output: Vec<AstResult>,
    components: HashMap<String, Component>,
}

impl Parser {
    /// Generates Parser from Lexer
    pub fn new(lexemes: Vec<Lexeme>) -> Self {
        let mut parser = Parser::new_parser(lexemes);
        loop {
            match parser.parse_token() {
                Err(Eof) => break,
                token => parser.push(token),
            }
        }
        parser
    }

    fn new_parser(lexemes: Vec<Lexeme>) -> Self {
        Parser {
            input: lexemes.into_iter().peekable(),
            output: Vec::new(),
            components: HashMap::new(),
        }
    }

    /// Pushes a new AstResult onto the output Vector
    fn push(&mut self, token: AstResult) {
        self.output.push(token);
    }

    /// A wrapper function around the input. taking the next element from the iterator.
    fn take(&mut self) -> Option<Lexeme> {
        self.input.next()
    }
    /// Performs a lookahead of the iterator.
    // This function should probably be refactored to not clone a token every time it's called.
    // Currently if you replace it with a reference, it creates a borrow, that messes up the
    // parser's current borrow structure.
    fn peek(&mut self) -> Option<Lexeme> {
        match self.input.peek() {
            Some(token) => Some(token.clone()),
            None => None,
        }
    }
    /// Output result vector
    pub fn output(self) -> Vec<AstResult> {
        self.output
    }
    /// Get all the components the parser found.
    pub fn get_components(&self) -> HashMap<String, Component> {
        self.components.clone()
    }

    /// Only parse components out of the source.
    pub fn component_pass(lexemes: Vec<Lexeme>) -> HashMap<String, Component> {
        let mut parser = Parser::new_parser(lexemes);
        loop {
            match parser.take() {
                Some(Symbol(index, Ampersand)) => {
                    let _ = parser.parse_component(true, index);
                }
                None => break,
                _ => {}
            }
        }

        parser.components
    }

    fn parse_component(&mut self, allow_definition: bool, index: usize) -> AstResult {
        let name = get_namespaced_identifer!(self, index, InvalidComponent, Ampersand);
        let mut component = Component::new(name);

        while let Some(token) = self.peek() {
            match token {
                Symbol(_, OpenParam) => {
                    let _ = self.take();
                    while let Some(token) = self.take() {
                        match token {
                            Symbol(index, At) => {

                                let identifier = get_identifer!(self.take(),
                                                                index,
                                                                UnexpectedToken);
                                component.add_arg_value(identifier);
                            }
                            Symbol(_, CloseParam) => {
                                match self.peek() {
                                    Some(Symbol(_, OpenBrace)) => break,
                                    _ => {
                                        return Ok(CompCall((ComponentCall::from_component(component))));
                                    }
                                }
                            }
                            Symbol(_, Comma) => {}
                            unexpected_token => return Err(UnexpectedToken(unexpected_token)),
                        }
                    }
                }
                token @ Symbol(_, OpenBrace) => {
                    let _ = self.take();
                    if allow_definition {
                        get_children!(self.take(), component);
                        break;
                    } else {
                        return Err(ExpectedCompCall(token));
                    }
                }
                unexpected_token => return Err(UnexpectedToken(unexpected_token)),
            }
        }
        if allow_definition {
            self.components.insert(component.name().into(), component);
            Ok(Text(String::new()))
        } else {
            // This unreachable, because with allow_definition = false, we should either get a
            // CompCall, or a ExpectedCompCall error.
            unreachable!()
        }
    }

    fn parse_element(&mut self, index: usize) -> AstResult {
        let tag = get_identifer!(self.take(), index, InvalidElement);
        let mut element = Element::new(tag.trim().to_owned());

        'element: while let Some(token) = self.take() {
            match token {
                Symbol(index, Ampersand) => {
                    let identifier = get_namespaced_identifer!(self,
                                                               index,
                                                               ExpectedCompCall,
                                                               Ampersand);
                    let mut component_call = ComponentCall::new(identifier);

                    if let Some(Symbol(_, OpenParam)) = self.peek() {
                        let _ = self.take();
                        while let Some(symbol) = self.take() {
                            match symbol {
                                Symbol(_, CloseParam) => break,
                                Symbol(index, At) => {
                                    let identifier = get_identifer!(self.take(),
                                                                    index,
                                                                    ExpectedVariable);
                                    component_call.add_value(identifier);
                                }
                                Symbol(_, Comma) => {}
                                unexpected_token => return Err(UnexpectedToken(unexpected_token)),
                            }
                        }
                    }
                    element.add_resource(component_call)
                }
                Symbol(index, OpenParam) => {
                    while let Some(token) = self.take() {
                        match token {
                            Symbol(_, CloseParam) => {
                                match self.peek() {
                                    Some(Symbol(_, OpenBrace)) => break,
                                    _ => return Ok(Html(element)),
                                }
                            }
                            Symbol(_, Quote) => {
                                let key = format!("{}{}{}", '"', self.read_leading_quotes(), '"');
                                element.add_attribute(key, String::from(""));
                            }
                            Word(_, key) => {
                                let value = match self.peek() {
                                    Some(Symbol(index, Equals)) => {
                                        let _ = self.take();
                                        match self.take() {
                                            Some(Word(_, text)) => text,
                                            Some(Symbol(_, Quote)) => self.read_leading_quotes(),
                                            Some(unexpected_token) => {
                                                return Err(InvalidTokenInAttributes(unexpected_token));
                                            }
                                            None => {
                                                return unexpected_eof!(Symbol(index, Equals));
                                            }
                                        }
                                    }
                                    Some(Word(_, _)) => String::from(""),
                                    Some(Symbol(_, CloseParam)) => String::from(""),
                                    Some(Symbol(_, Quote)) => String::from(""),
                                    Some(invalid_token) => {
                                        return Err(InvalidTokenInAttributes(invalid_token))
                                    }
                                    None => return unexpected_eof!(Word(index, key)),
                                };

                                element.add_attribute(key, value);
                            }
                            invalid_token => return Err(InvalidTokenInAttributes(invalid_token)),
                        }
                    }
                }
                Symbol(index, Dot) => {
                    match self.take() {
                        Some(Word(_, class)) => element.add_class(class),
                        Some(unexpected_token) => {
                            return Err(NoNameAttachedToClass(unexpected_token))
                        }
                        None => return Err(UnexpectedEof(Symbol(index, Dot))),
                    }
                }
                Symbol(index, Pound) => {
                    match self.take() {
                        Some(Word(_, id)) => element.add_attribute(String::from("id"), id),
                        Some(unexpected_token) => return Err(NoNameAttachedToId(unexpected_token)),
                        None => return Err(UnexpectedEof(Symbol(index, Pound))),
                    }
                }
                Symbol(_, OpenBrace) => {
                    get_children!(self.take(), element);
                    break;
                }
                unexpected_token => return Err(UnexpectedToken(unexpected_token)),
            }
        }
        Ok(Html(element))
    }

    fn parse_escaped(&mut self) -> AstResult {
        match self.peek() {
            Some(Symbol(_, ref operator)) => {
                let _ = self.take();
                Ok(Text(operator.to_string()))
            }
            Some(_) => Ok(Text(String::new())),
            None => Err(Eof),
        }
    }

    fn parse_function(&mut self, index: usize) -> AstResult {
        let identifier = get_namespaced_identifer!(self, index, InvalidFunctionCall, Dollar);
        let mut func_call = FunctionCall::new(identifier);

        match self.take() {
            Some(Symbol(_, OpenParam)) => {
                while let Some(token) = self.take() {
                    match token {
                        Word(index, arg_name) => {
                            match self.take() {
                                Some(Symbol(index, Equals)) => {
                                    match self.take() {
                                        Some(Symbol(index, At)) => {
                                            match self.take() {
                                                Some(Word(_, identifier)) => {
                                                    func_call.add_value_arg(arg_name, identifier);
                                                }
                                                Some(unexpected_token) => {
                                                    return Err(ExpectedVariable(unexpected_token))
                                                }
                                                None => unexpected_eof!(Symbol(index, At)),
                                            }
                                        }
                                        Some(Symbol(index, Ampersand)) => {
                                            match self.take() {
                                                Some(Word(_, identifier)) => {
                                                    func_call.add_component_arg(arg_name,
                                                                                identifier);
                                                }
                                                Some(unexpected_token) => {
                                                    return Err(ExpectedCompCall(unexpected_token))
                                                }
                                                None => unexpected_eof!(Symbol(index, Ampersand)),
                                            }
                                        }
                                        Some(unexpected_token) => {
                                            return Err(UnexpectedToken(unexpected_token))
                                        }
                                        None => unexpected_eof!(Symbol(index, Equals)),

                                    }
                                }
                                Some(unexpected_token) => {
                                    return Err(InvalidFunctionCall(unexpected_token))
                                }
                                None => unexpected_eof!(Word(index, arg_name)),

                            }
                        }
                        Symbol(_, CloseParam) => break,
                        Symbol(_, Comma) => {}
                        unexpected_token => return Err(UnexpectedToken(unexpected_token)),
                    }
                }
            }
            Some(unexpected_token) => return Err(InvalidFunctionCall(unexpected_token)),
            None => unexpected_eof!(Symbol(index, Dollar)),
        }
        Ok(Function(func_call))
    }



    fn parse_text(&mut self, word: String) -> AstResult {
        let mut text = String::from(word);
        loop {
            let peek = self.peek();
            match peek {
                Some(Word(_, ref peek_text)) => {
                    text.push_str(&*peek_text);
                    let _ = self.take();
                }
                _ => return Ok(Text(text)),
            }
        }
    }

    /// 
    fn parse_token(&mut self) -> AstResult {
        match self.take() {
            // concatenate all the word tokens that are adjacent to each other into a single "Text"
            // token.
            Some(Word(_, word)) => self.parse_text(word),
            Some(Symbol(index, At)) => {
                Ok(Variable(get_namespaced_identifer!(self, index, ExpectedVariable, At)))
            }
            Some(Symbol(index, ForwardSlash)) => self.parse_element(index),
            Some(Symbol(_, BackSlash)) => self.parse_escaped(),
            Some(Symbol(index, Ampersand)) => self.parse_component(true, index),
            Some(Symbol(index, Dollar)) => self.parse_function(index),
            Some(Symbol(_, operator)) => Ok(Text(operator.to_string())),
            None => Err(Eof),
        }
    }

    /// turns all Operators into text until it it reaches the first " or Quote operator.
    fn read_leading_quotes(&mut self) -> String {
        let mut value = String::new();
        while let Some(token) = self.take() {
            match token {
                Symbol(_, Quote) => break,
                Word(_, text) => value.push_str(&*text),
                Symbol(_, operator) => value.push_str(&*operator.to_string()),
            }
        }
        value
    }
}
