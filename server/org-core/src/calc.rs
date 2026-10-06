//! The part of Emacs Calc that Org's table formulas use: algebraic
//! formulas as `calc-eval` reads and evaluates them under
//! `org-calc-default-modes`, and their results as Calc writes them.
//!
//! - Integers are exact; a division that does not come out even gives a
//!   float, as with `calc-prefer-frac` off.
//! - Floats print with 8 significant digits (`calc-float-format` is
//!   `(float 8)`), a whole one with a trailing `.` (`5.`), in scientific
//!   notation when the point would be more than 12 digits to the right or
//!   more than one zero to the left (`1e-3`, `1.5e12`).
//! - `*` binds tighter than `/`, so `a/b*c` is `a/(b*c)`; juxtaposition
//!   multiplies (`2 x`, `2(3)`); `^` is right-associative and binds
//!   tighter than unary minus.
//! - Angles are in degrees.
//! - A name that is not a number stays a symbol: `x + 2` is written back
//!   as such, with constants gathered, `x*2` as `2 x`, `x - x` as `0`.

use std::convert::Infallible;
use std::fmt::Write as _;

/// A number as Calc keeps it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Number {
    Int(i128),
    Float(f64),
}

impl Number {
    fn as_f64(self) -> f64 {
        match self {
            Number::Int(n) => n as f64,
            Number::Float(f) => f,
        }
    }

    fn is_zero(self) -> bool {
        self.as_f64() == 0.0
    }
}

/// Calc's floats keep 12 significant digits (`calc-internal-prec`).
fn float(value: f64) -> Number {
    if value == 0.0 || !value.is_finite() {
        return Number::Float(value);
    }
    let rounded: f64 = format!("{value:.11e}").parse().unwrap_or(value);
    Number::Float(rounded)
}

/// What a formula evaluates to.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Number(Number),
    Vector(Vec<Value>),
    /// What Calc leaves as a formula: a name, or an expression with one.
    Symbolic(Expr),
    Nan,
    /// A date form, `<2024-01-01 Mon>`: the day number (day 1 is
    /// 0001-01-01), with the time of day as a fraction when it has one.
    Date(Number),
}

/// A symbolic expression, kept to be written back.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Name(String),
    Number(Number),
    /// A sum of terms, each added (`true`) or subtracted.
    Sum(Vec<(bool, Expr)>),
    Product(Box<Expr>, Box<Expr>),
    Quotient(Box<Expr>, Box<Expr>),
    Power(Box<Expr>, Box<Expr>),
    Negative(Box<Expr>),
    /// An exact fraction Calc keeps as it is: a division of integers by
    /// zero (`1/0`).
    Ratio(i128, i128),
    Date(Number),
    Call(String, Vec<Expr>),
}

/// Calc's float display: with `digits` significant digits, `fix`ed
/// decimals or in `sci`entific notation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloatFormat {
    Float(usize),
    Fix(usize),
    Sci(usize),
}

/// The modes a formula can set after its `;`.
#[derive(Debug, Clone, Copy)]
pub struct Modes {
    pub float_format: FloatFormat,
    pub radians: bool,
    /// The result must be a real number (`calc-eval` with `num`).
    pub number_result: bool,
}

impl Default for Modes {
    fn default() -> Self {
        Modes { float_format: FloatFormat::Float(8), radians: false, number_result: false }
    }
}

/// Evaluate `formula` as `calc-eval` does, and write the result as Calc
/// would. `Err` for what Calc cannot read (`#ERROR` in a table).
pub fn eval(formula: &str, modes: &Modes) -> Result<String, String> {
    let tokens = tokenize(formula)?;
    let mut parser = Parser { tokens, at: 0, modes };
    let value = parser.expression(0)?;
    if parser.at < parser.tokens.len() {
        return Err(format!("Unexpected {:?}", parser.tokens[parser.at]));
    }
    let Ok(value) = value;
    if modes.number_result && !matches!(value, Value::Number(_)) {
        return Err("Number expected".to_string());
    }
    Ok(write_value(&value, modes))
}

// ----- tokens

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(Number),
    Date(Number),
    Name(String),
    Operator(&'static str),
    Open(char),
    Close(char),
    Comma,
}

const OPERATORS: &[&str] =
    &["**", "<=", ">=", "==", "!=", "&&", "||", "+", "-", "*", "/", "%", "\\", "^", "<", ">", "=", "!"];

fn tokenize(text: &str) -> Result<Vec<Token>, String> {
    let chars: Vec<char> = text.chars().collect();
    let mut tokens = Vec::new();
    let mut at = 0;
    while at < chars.len() {
        let c = chars[at];
        if c.is_whitespace() {
            at += 1;
            continue;
        }
        if c.is_ascii_digit() || (c == '.' && chars.get(at + 1).is_some_and(char::is_ascii_digit)) {
            let start = at;
            while at < chars.len() && chars[at].is_ascii_digit() {
                at += 1;
            }
            let mut is_float = false;
            if at < chars.len() && chars[at] == '.' {
                is_float = true;
                at += 1;
                while at < chars.len() && chars[at].is_ascii_digit() {
                    at += 1;
                }
            }
            // An exponent: `1e3`, `1.5e-7`.
            if at < chars.len() && (chars[at] == 'e' || chars[at] == 'E') {
                let mut end = at + 1;
                if end < chars.len() && (chars[end] == '-' || chars[end] == '+') {
                    end += 1;
                }
                if end < chars.len() && chars[end].is_ascii_digit() {
                    while end < chars.len() && chars[end].is_ascii_digit() {
                        end += 1;
                    }
                    is_float = true;
                    at = end;
                }
            }
            let literal: String = chars[start..at].iter().collect();
            let number = if is_float {
                Number::Float(literal.parse::<f64>().map_err(|_| format!("Bad number {literal}"))?)
            } else {
                match literal.parse::<i128>() {
                    Ok(n) => Number::Int(n),
                    Err(_) => float(literal.parse::<f64>().map_err(|_| format!("Bad number {literal}"))?),
                }
            };
            tokens.push(Token::Number(number));
            continue;
        }
        if c.is_alphabetic() || c == '_' {
            let start = at;
            while at < chars.len() && (chars[at].is_alphanumeric() || chars[at] == '_') {
                at += 1;
            }
            tokens.push(Token::Name(chars[start..at].iter().collect()));
            continue;
        }
        if c == '<' {
            if let Some((date, length)) = date_form(&chars[at..])? {
                tokens.push(Token::Date(date));
                at += length;
                continue;
            }
        }
        match c {
            '(' | '[' => tokens.push(Token::Open(c)),
            ')' | ']' => tokens.push(Token::Close(c)),
            ',' => tokens.push(Token::Comma),
            _ => {
                let rest: String = chars[at..chars.len().min(at + 2)].iter().collect();
                let Some(operator) = OPERATORS.iter().find(|operator| rest.starts_with(**operator)) else {
                    return Err(format!("Bad character {c}"));
                };
                tokens.push(Token::Operator(operator));
                at += operator.chars().count();
                continue;
            }
        }
        at += 1;
    }
    Ok(tokens)
}

/// A date form at the start of `chars`, `<2024-01-01 Mon>` or
/// `<2024-01-01 Mon 10:30>`, and its length.
fn date_form(chars: &[char]) -> Result<Option<(Number, usize)>, String> {
    let Some(close) = chars.iter().position(|c| *c == '>') else { return Ok(None) };
    let inside: String = chars[1..close].iter().collect();
    let bytes = inside.as_bytes();
    let digits = |range: std::ops::Range<usize>| {
        bytes
            .get(range.clone())
            .filter(|part| part.iter().all(u8::is_ascii_digit))
            .map(|_| inside[range].parse::<i64>().unwrap_or(0))
    };
    let (Some(year), Some(month), Some(day)) = (digits(0..4), digits(5..7), digits(8..10)) else { return Ok(None) };
    if bytes.get(4) != Some(&b'-') || bytes.get(7) != Some(&b'-') {
        return Ok(None);
    }
    if !(1..=12).contains(&month) {
        return Err("Month value is out of range".to_string());
    }
    if !(1..=crate::date::days_in_month(year, month)).contains(&day) {
        return Err("Day value is out of range".to_string());
    }
    let number = crate::date::Date::new(year, month, day).to_day();
    let time = inside[10..].split_whitespace().find_map(|word| {
        let (hour, minute) = word.split_once(':')?;
        let minute = minute.get(..2)?;
        Some((hour.parse::<i64>().ok()?, minute.parse::<i64>().ok()?))
    });
    let date = match time {
        Some((hour, minute)) => float(number as f64 + (hour * 3600 + minute * 60) as f64 / 86400.0),
        None => Number::Int(i128::from(number)),
    };
    Ok(Some((date, close + 1)))
}

// ----- parsing and evaluating

/// Evaluation itself does not fail: what Calc cannot compute stays a
/// formula.
type Outcome = Result<Value, Infallible>;

struct Parser<'a> {
    tokens: Vec<Token>,
    at: usize,
    modes: &'a Modes,
}

/// Binding powers: left, and right for the operand after.
fn infix(token: &Token) -> Option<(u8, u8, &'static str)> {
    let Token::Operator(operator) = token else { return None };
    Some(match *operator {
        "^" | "**" => (201, 200, "^"),
        "*" => (196, 196, "*"),
        "/" => (190, 191, "/"),
        "%" => (190, 191, "%"),
        "\\" => (190, 191, "\\"),
        "+" => (180, 181, "+"),
        "-" => (180, 181, "-"),
        "<" | ">" | "<=" | ">=" | "==" | "!=" | "=" => (160, 161, operator),
        "&&" => (110, 111, "&&"),
        "||" => (100, 101, "||"),
        _ => return None,
    })
}

/// Whether a token can start an operand: juxtaposition multiplies then.
fn starts_operand(token: &Token) -> bool {
    matches!(token, Token::Number(_) | Token::Date(_) | Token::Name(_) | Token::Open(_))
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at)
    }

    fn next(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.at).cloned();
        self.at += 1;
        token
    }

    fn expect(&mut self, wanted: Token) -> Result<(), String> {
        match self.next() {
            Some(token) if token == wanted => Ok(()),
            other => Err(format!("Expected {wanted:?}, found {other:?}")),
        }
    }

    /// An expression whose operators bind tighter than `min`.
    fn expression(&mut self, min: u8) -> Result<Outcome, String> {
        let mut left = self.prefix()?;
        while let Some(token) = self.peek().cloned() {
            let (left_power, right_power, operator) = match infix(&token) {
                Some(found) => found,
                // `2 x`, `2(3)`: as `*`.
                None if starts_operand(&token) => (196, 196, "juxtaposition"),
                None => break,
            };
            if left_power < min {
                break;
            }
            if operator != "juxtaposition" {
                self.at += 1;
            }
            let right = self.expression(right_power + u8::from(operator == "*" || operator == "juxtaposition"))?;
            let operator = if operator == "juxtaposition" { "*" } else { operator };
            left = match (left, right) {
                (Ok(left), Ok(right)) => binary(operator, left, right),
                (Err(fault), _) | (_, Err(fault)) => Err(fault),
            };
        }
        Ok(left)
    }

    fn prefix(&mut self) -> Result<Outcome, String> {
        match self.next() {
            Some(Token::Number(number)) => Ok(Ok(Value::Number(number))),
            Some(Token::Date(date)) => Ok(Ok(Value::Date(date))),
            Some(Token::Operator("-")) => {
                // Below `^`: `-2^2` is `-(2^2)`.
                let operand = self.expression(197)?;
                Ok(operand.map(negate))
            }
            Some(Token::Operator("+")) => self.expression(197),
            Some(Token::Operator("!")) => {
                let operand = self.expression(197)?;
                Ok(operand.map(|value| match number_of(&value) {
                    Some(number) => Value::Number(Number::Int(i128::from(number.is_zero()))),
                    None => Value::Symbolic(Expr::Call("!".to_string(), vec![to_expr(&value)])),
                }))
            }
            Some(Token::Open('(')) => {
                let inner = self.expression(0)?;
                self.expect(Token::Close(')'))?;
                Ok(inner)
            }
            Some(Token::Open('[')) => {
                let mut items = Vec::new();
                if self.peek() != Some(&Token::Close(']')) {
                    loop {
                        items.push(self.expression(0)?);
                        match self.next() {
                            Some(Token::Comma) => continue,
                            Some(Token::Close(']')) => break,
                            other => return Err(format!("Expected , or ], found {other:?}")),
                        }
                    }
                } else {
                    self.at += 1;
                }
                let items: Result<Vec<Value>, Infallible> = items.into_iter().collect();
                Ok(items.map(Value::Vector))
            }
            Some(Token::Name(name)) => {
                if self.peek() == Some(&Token::Open('(')) {
                    self.at += 1;
                    let mut arguments = Vec::new();
                    if self.peek() != Some(&Token::Close(')')) {
                        loop {
                            arguments.push(self.expression(0)?);
                            match self.next() {
                                Some(Token::Comma) => continue,
                                Some(Token::Close(')')) => break,
                                other => return Err(format!("Expected , or ), found {other:?}")),
                            }
                        }
                    } else {
                        self.at += 1;
                    }
                    let arguments: Result<Vec<Value>, Infallible> = arguments.into_iter().collect();
                    return Ok(match arguments {
                        Ok(arguments) => call(&name, arguments, self.modes),
                        Err(fault) => Err(fault),
                    });
                }
                Ok(Ok(match name.as_str() {
                    "nan" => Value::Nan,
                    _ => Value::Symbolic(Expr::Name(name)),
                }))
            }
            other => Err(format!("Unexpected {other:?}")),
        }
    }
}

fn number_of(value: &Value) -> Option<Number> {
    match value {
        Value::Number(number) => Some(*number),
        _ => None,
    }
}

fn negate(value: Value) -> Value {
    match value {
        Value::Number(Number::Int(n)) => Value::Number(Number::Int(-n)),
        Value::Number(Number::Float(f)) => Value::Number(Number::Float(-f)),
        Value::Vector(items) => Value::Vector(items.into_iter().map(negate).collect()),
        Value::Nan => Value::Nan,
        Value::Symbolic(expr) => Value::Symbolic(simplify_negative(expr)),
        Value::Date(date) => Value::Symbolic(Expr::Negative(Box::new(Expr::Date(date)))),
    }
}

fn simplify_negative(expr: Expr) -> Expr {
    match expr {
        Expr::Negative(inner) => *inner,
        Expr::Number(number) => Expr::Number(match number {
            Number::Int(n) => Number::Int(-n),
            Number::Float(f) => Number::Float(-f),
        }),
        other => Expr::Negative(Box::new(other)),
    }
}

/// Apply an operator to two values: on numbers, element by element on
/// vectors, kept as an expression on symbols.
fn binary(operator: &str, left: Value, right: Value) -> Outcome {
    match (left, right) {
        (Value::Nan, _) | (_, Value::Nan) => Ok(Value::Nan),
        (Value::Vector(items), Value::Vector(others)) if items.len() == others.len() => Ok(Value::Vector(
            items.into_iter().zip(others).map(|(a, b)| binary(operator, a, b)).collect::<Result<_, _>>()?,
        )),
        (Value::Vector(items), other) => {
            Ok(Value::Vector(items.into_iter().map(|a| binary(operator, a, other.clone())).collect::<Result<_, _>>()?))
        }
        (other, Value::Vector(items)) => {
            Ok(Value::Vector(items.into_iter().map(|b| binary(operator, other.clone(), b)).collect::<Result<_, _>>()?))
        }
        (Value::Number(a), Value::Number(b)) => arithmetic(operator, a, b),
        // Dates: a date and days make a date, two dates the days between.
        (Value::Date(a), Value::Date(b)) if matches!(operator, "+" | "-") => arithmetic(operator, a, b),
        (Value::Date(a), Value::Number(b)) if matches!(operator, "+" | "-") => Ok(dated(arithmetic(operator, a, b)?)),
        (Value::Number(a), Value::Date(b)) if operator == "+" => Ok(dated(arithmetic(operator, a, b)?)),
        (Value::Date(a) | Value::Number(a), Value::Date(b) | Value::Number(b)) if is_comparison(operator) => {
            arithmetic(operator, a, b)
        }
        (left, right) => Ok(symbolic(operator, to_expr(&left), to_expr(&right))),
    }
}

fn is_comparison(operator: &str) -> bool {
    matches!(operator, "<" | ">" | "<=" | ">=" | "==" | "=" | "!=")
}

/// A number of days as a date.
fn dated(value: Value) -> Value {
    match value {
        Value::Number(number) => Value::Date(number),
        other => other,
    }
}

fn arithmetic(operator: &str, a: Number, b: Number) -> Outcome {
    use Number::Int;
    let truth = |value: bool| Ok(Value::Number(Int(i128::from(value))));
    let result = match operator {
        "+" => match (a, b) {
            (Int(x), Int(y)) => x.checked_add(y).map_or_else(|| float(x as f64 + y as f64), Int),
            _ => float(a.as_f64() + b.as_f64()),
        },
        "-" => match (a, b) {
            (Int(x), Int(y)) => x.checked_sub(y).map_or_else(|| float(x as f64 - y as f64), Int),
            _ => float(a.as_f64() - b.as_f64()),
        },
        "*" => match (a, b) {
            (Int(x), Int(y)) => x.checked_mul(y).map_or_else(|| float(x as f64 * y as f64), Int),
            _ => float(a.as_f64() * b.as_f64()),
        },
        "/" => {
            if b.is_zero() {
                return Ok(match (a, b) {
                    (Int(x), Int(y)) => Value::Symbolic(Expr::Ratio(x, y)),
                    _ => Value::Symbolic(Expr::Quotient(Box::new(Expr::Number(a)), Box::new(Expr::Number(b)))),
                });
            }
            match (a, b) {
                (Int(x), Int(y)) if x % y == 0 => Int(x / y),
                _ => float(a.as_f64() / b.as_f64()),
            }
        }
        "%" => {
            if b.is_zero() {
                return Ok(symbolic("%", Expr::Number(a), Expr::Number(b)));
            }
            // The sign of the divisor, as Calc's `mod`.
            match (a, b) {
                (Int(x), Int(y)) => Int(x.rem_euclid(y) + if y < 0 && x.rem_euclid(y) != 0 { y } else { 0 }),
                _ => {
                    let (x, y) = (a.as_f64(), b.as_f64());
                    float(x - y * (x / y).floor())
                }
            }
        }
        "\\" => {
            if b.is_zero() {
                return Ok(symbolic("\\", Expr::Number(a), Expr::Number(b)));
            }
            Int((a.as_f64() / b.as_f64()).floor() as i128)
        }
        "^" => power(a, b),
        "<" => return truth(a.as_f64() < b.as_f64()),
        ">" => return truth(a.as_f64() > b.as_f64()),
        "<=" => return truth(a.as_f64() <= b.as_f64()),
        ">=" => return truth(a.as_f64() >= b.as_f64()),
        "==" | "=" => return truth(a.as_f64() == b.as_f64()),
        "!=" => return truth(a.as_f64() != b.as_f64()),
        "&&" => return truth(!a.is_zero() && !b.is_zero()),
        "||" => return truth(!a.is_zero() || !b.is_zero()),
        _ => return Ok(Value::Nan),
    };
    Ok(Value::Number(result))
}

fn power(a: Number, b: Number) -> Number {
    match (a, b) {
        (Number::Int(x), Number::Int(y)) if y >= 0 => u32::try_from(y)
            .ok()
            .and_then(|y| x.checked_pow(y))
            .map_or_else(|| float((x as f64).powf(y as f64)), Number::Int),
        _ => float(a.as_f64().powf(b.as_f64())),
    }
}

/// Integer results of functions that come out exactly.
fn exact(value: f64) -> Number {
    if value.is_finite() && value.fract() == 0.0 && value.abs() < 1e15 {
        Number::Int(value as i128)
    } else {
        float(value)
    }
}

/// The numbers of the arguments, vectors flattened.
fn flatten(arguments: &[Value]) -> Vec<Value> {
    let mut out = Vec::new();
    for argument in arguments {
        match argument {
            Value::Vector(items) => out.extend(flatten(items)),
            other => out.push(other.clone()),
        }
    }
    out
}

/// A Calc function.
fn call(name: &str, arguments: Vec<Value>, modes: &Modes) -> Outcome {
    let keep = |arguments: &[Value]| {
        Ok(Value::Symbolic(Expr::Call(name.to_string(), arguments.iter().map(to_expr).collect())))
    };
    let degrees = |x: f64| if modes.radians { x } else { x.to_radians() };
    let from_radians = |x: f64| if modes.radians { x } else { x.to_degrees() };
    // Functions of vectors.
    if let Some(reduce) = name.strip_prefix('v') {
        let items = flatten(&arguments);
        if matches!(reduce, "len" | "count") {
            return Ok(Value::Number(Number::Int(items.len() as i128)));
        }
        if items.iter().any(|item| matches!(item, Value::Nan)) {
            return Ok(Value::Nan);
        }
        match reduce {
            "sum" => {
                return items.into_iter().try_fold(Value::Number(Number::Int(0)), |sum, item| binary("+", sum, item))
            }
            "prod" => {
                return items
                    .into_iter()
                    .try_fold(Value::Number(Number::Int(1)), |product, item| binary("*", product, item))
            }
            "max" | "min" if !items.is_empty() && items.iter().all(|item| matches!(item, Value::Date(_))) => {
                let date = |item: &Value| match item {
                    Value::Date(date) => *date,
                    _ => unreachable!("dates"),
                };
                let pick = |best: Number, x: Number| {
                    if (reduce == "max") == (x.as_f64() > best.as_f64()) && x.as_f64() != best.as_f64() {
                        x
                    } else {
                        best
                    }
                };
                return Ok(Value::Date(items.iter().map(date).reduce(pick).expect("not empty")));
            }
            "max" | "min" | "mean" | "median" | "sdev" | "psdev" | "var" | "pvar" => {
                let numbers: Option<Vec<Number>> = items.iter().map(number_of).collect();
                let Some(numbers) = numbers.filter(|numbers| !numbers.is_empty()) else { return keep(&arguments) };
                return statistics(reduce, numbers);
            }
            _ => {}
        }
    }
    // The parts of a date; a number is a day number.
    if let [Value::Date(date) | Value::Number(date)] = arguments.as_slice() {
        let value = date.as_f64();
        let day = value.floor() as i64;
        let seconds = ((value - value.floor()) * 86400.0).round() as i64;
        let civil = crate::date::Date::from_day(day);
        let part = match name {
            "year" => Some(civil.year),
            "month" => Some(civil.month),
            "day" => Some(civil.day),
            "weekday" => Some(crate::date::weekday(day) as i64),
            "hour" => Some(seconds / 3600),
            "minute" => Some(seconds % 3600 / 60),
            "second" => Some(seconds % 60),
            _ => None,
        };
        if let Some(part) = part {
            return Ok(Value::Number(Number::Int(i128::from(part))));
        }
    }
    let numbers: Option<Vec<Number>> = arguments.iter().map(number_of).collect();
    let Some(numbers) = numbers else {
        // `[1,2]` given to a function of numbers: each element.
        if arguments.len() == 1 {
            if let Value::Vector(items) = &arguments[0] {
                return Ok(Value::Vector(
                    items.iter().map(|item| call(name, vec![item.clone()], modes)).collect::<Result<_, _>>()?,
                ));
            }
        }
        return keep(&arguments);
    };
    let one = |f: fn(f64) -> f64| numbers.first().map(|x| f(x.as_f64()));
    let value = match (name, numbers.as_slice()) {
        ("abs", [x]) => match x {
            Number::Int(n) => Number::Int(n.abs()),
            Number::Float(f) => Number::Float(f.abs()),
        },
        ("sqrt", [x]) => {
            let root = x.as_f64().sqrt();
            if x.as_f64() < 0.0 {
                return keep(&arguments);
            }
            match x {
                Number::Int(n) if (root.round() as i128).pow(2) == *n => Number::Int(root.round() as i128),
                _ => float(root),
            }
        }
        ("exp", [x]) => {
            if x.is_zero() {
                Number::Int(1)
            } else {
                float(x.as_f64().exp())
            }
        }
        ("ln", [x]) => {
            if x.as_f64() == 1.0 {
                Number::Int(0)
            } else {
                float(x.as_f64().ln())
            }
        }
        ("log10", [x]) => match x {
            Number::Int(_) => exact(x.as_f64().log10()),
            _ => float(x.as_f64().log10()),
        },
        ("log", [x, base]) => {
            let value = x.as_f64().ln() / base.as_f64().ln();
            match (x, base) {
                (Number::Int(_), Number::Int(_)) if (value.round() - value).abs() < 1e-12 => {
                    Number::Int(value.round() as i128)
                }
                _ => float(value),
            }
        }
        ("log", [x]) => float(x.as_f64().ln()),
        // At zero, exact in radians too.
        ("sin" | "tan", [Number::Int(0)]) => Number::Int(0),
        ("cos", [Number::Int(0)]) => Number::Int(1),
        ("sin" | "cos", [x]) => {
            let angle = x.as_f64();
            let value = if name == "sin" { degrees(angle).sin() } else { degrees(angle).cos() };
            // Multiples of a right angle come out exact.
            if !modes.radians && matches!(x, Number::Int(_)) && (angle % 90.0) == 0.0 {
                exact(value.round())
            } else {
                float(value)
            }
        }
        ("tan", [x]) => float(degrees(x.as_f64()).tan()),
        ("arcsin", [x]) => exact_or_float(from_radians(x.as_f64().asin())),
        ("arccos", [x]) => exact_or_float(from_radians(x.as_f64().acos())),
        ("arctan", [x]) => exact_or_float(from_radians(x.as_f64().atan())),
        ("round", [x]) => Number::Int(x.as_f64().round() as i128),
        ("round", [x, digits]) => {
            let scale = 10f64.powi(digits.as_f64() as i32);
            float((x.as_f64() * scale).round() / scale)
        }
        ("floor", [x]) => Number::Int(x.as_f64().floor() as i128),
        ("ceil", [x]) => Number::Int(x.as_f64().ceil() as i128),
        ("trunc", [x]) => Number::Int(x.as_f64().trunc() as i128),
        ("max", [_, ..]) => {
            *numbers.iter().fold(&numbers[0], |best, x| if x.as_f64() > best.as_f64() { x } else { best })
        }
        ("min", [_, ..]) => {
            *numbers.iter().fold(&numbers[0], |best, x| if x.as_f64() < best.as_f64() { x } else { best })
        }
        ("mod", [x, y]) => return arithmetic("%", *x, *y),
        ("idiv", [x, y]) => return arithmetic("\\", *x, *y),
        ("fact", [Number::Int(n)]) if *n >= 0 && *n <= 33 => Number::Int((1..=*n).product()),
        ("if", [condition, then, otherwise]) => {
            if condition.is_zero() {
                *otherwise
            } else {
                *then
            }
        }
        ("gcd", [Number::Int(a), Number::Int(b)]) => {
            let (mut a, mut b) = (a.abs(), b.abs());
            while b != 0 {
                (a, b) = (b, a % b);
            }
            Number::Int(a)
        }
        _ => {
            let _ = one;
            return keep(&arguments);
        }
    };
    Ok(Value::Number(value))
}

fn exact_or_float(value: f64) -> Number {
    if (value.round() - value).abs() < 1e-9 {
        Number::Int(value.round() as i128)
    } else {
        float(value)
    }
}

/// `vmax`, `vmean`, `vmedian`, `vsdev` and their kin.
fn statistics(name: &str, numbers: Vec<Number>) -> Outcome {
    let sum = |numbers: &[Number]| {
        numbers.iter().try_fold(Value::Number(Number::Int(0)), |sum, x| binary("+", sum, Value::Number(*x)))
    };
    let count = Value::Number(Number::Int(numbers.len() as i128));
    match name {
        "max" => {
            Ok(Value::Number(
                *numbers.iter().fold(&numbers[0], |best, x| if x.as_f64() > best.as_f64() { x } else { best }),
            ))
        }
        "min" => {
            Ok(Value::Number(
                *numbers.iter().fold(&numbers[0], |best, x| if x.as_f64() < best.as_f64() { x } else { best }),
            ))
        }
        "mean" => binary("/", sum(&numbers)?, count),
        "median" => {
            let mut sorted = numbers.clone();
            sorted.sort_by(|a, b| a.as_f64().total_cmp(&b.as_f64()));
            let middle = sorted.len() / 2;
            if sorted.len() % 2 == 1 {
                Ok(Value::Number(sorted[middle]))
            } else {
                let both = binary("+", Value::Number(sorted[middle - 1]), Value::Number(sorted[middle]))?;
                binary("/", both, Value::Number(Number::Int(2)))
            }
        }
        _ => {
            let n = numbers.len() as f64;
            let mean = numbers.iter().map(|x| x.as_f64()).sum::<f64>() / n;
            let squares = numbers.iter().map(|x| (x.as_f64() - mean).powi(2)).sum::<f64>();
            let sample = !name.starts_with('p');
            let divisor = if sample { n - 1.0 } else { n };
            if divisor <= 0.0 {
                return Ok(Value::Number(Number::Int(0)));
            }
            let variance = squares / divisor;
            Ok(Value::Number(if name.ends_with("var") { exact_or_float(variance) } else { float(variance.sqrt()) }))
        }
    }
}

// ----- symbols

fn to_expr(value: &Value) -> Expr {
    match value {
        Value::Number(number) => Expr::Number(*number),
        Value::Symbolic(expr) => expr.clone(),
        Value::Vector(items) => Expr::Call("vector".to_string(), items.iter().map(to_expr).collect()),
        Value::Nan => Expr::Name("nan".to_string()),
        Value::Date(date) => Expr::Date(*date),
    }
}

/// An operation on a symbol, simplified as far as Calc's default
/// simplification goes in the common cases.
fn symbolic(operator: &str, left: Expr, right: Expr) -> Value {
    let expr = match operator {
        "+" | "-" => {
            let mut terms = match left {
                Expr::Sum(terms) => terms,
                other => vec![(true, other)],
            };
            let add = operator == "+";
            match right {
                Expr::Sum(more) => terms.extend(more.into_iter().map(|(sign, term)| (sign == add, term))),
                other => terms.push((add, other)),
            }
            return gather(terms);
        }
        "*" => match (left, right) {
            (Expr::Number(n), other) | (other, Expr::Number(n)) => {
                if n == Number::Int(1) {
                    other
                } else if n == Number::Int(0) {
                    Expr::Number(Number::Int(0))
                } else {
                    Expr::Product(Box::new(Expr::Number(n)), Box::new(other))
                }
            }
            (a, b) => Expr::Product(Box::new(a), Box::new(b)),
        },
        "/" => Expr::Quotient(Box::new(left), Box::new(right)),
        "^" => Expr::Power(Box::new(left), Box::new(right)),
        _ => Expr::Call(operator.to_string(), vec![left, right]),
    };
    Value::Symbolic(expr)
}

/// A sum with its numbers added up where the first number is, and terms
/// that cancel gone.
fn gather(terms: Vec<(bool, Expr)>) -> Value {
    let mut out: Vec<(bool, Expr)> = Vec::new();
    let mut constant: Option<usize> = None;
    for (sign, term) in terms {
        if let Expr::Number(n) = term {
            let n = if sign { n } else { negate_number(n) };
            match constant {
                Some(at) => {
                    let Expr::Number(old) = out[at].1 else { unreachable!() };
                    let old = if out[at].0 { old } else { negate_number(old) };
                    let total = match arithmetic("+", old, n) {
                        Ok(Value::Number(total)) => total,
                        _ => old,
                    };
                    out[at] = (true, Expr::Number(total));
                }
                None => {
                    constant = Some(out.len());
                    out.push((true, Expr::Number(n)));
                }
            }
            continue;
        }
        // `x - x`
        if let Some(at) = out.iter().position(|(other_sign, other)| *other == term && *other_sign != sign) {
            out.remove(at);
            if let Some(index) = constant.filter(|index| *index > at) {
                constant = Some(index - 1);
            }
            continue;
        }
        out.push((sign, term));
    }
    // A zero added is no term, unless it is all there is.
    if let Some(at) = constant {
        if out.len() > 1 && matches!(out[at].1, Expr::Number(n) if n.is_zero()) {
            out.remove(at);
        }
    }
    // Negative numbers are subtracted.
    let out: Vec<(bool, Expr)> = out
        .into_iter()
        .map(|(sign, term)| match term {
            Expr::Number(n) if n.as_f64() < 0.0 => (!sign, Expr::Number(negate_number(n))),
            other => (sign, other),
        })
        .collect();
    match out.len() {
        0 => Value::Number(Number::Int(0)),
        1 if out[0].0 => match out.into_iter().next().unwrap().1 {
            Expr::Number(n) => Value::Number(n),
            other => Value::Symbolic(other),
        },
        _ => Value::Symbolic(Expr::Sum(out)),
    }
}

fn negate_number(n: Number) -> Number {
    match n {
        Number::Int(n) => Number::Int(-n),
        Number::Float(f) => Number::Float(-f),
    }
}

// ----- writing

fn write_value(value: &Value, modes: &Modes) -> String {
    match value {
        Value::Number(number) => write_number(*number, modes.float_format),
        Value::Nan => "nan".to_string(),
        Value::Vector(items) => {
            format!("[{}]", items.iter().map(|item| write_value(item, modes)).collect::<Vec<_>>().join(", "))
        }
        Value::Symbolic(expr) => write_expr(expr, 0, modes),
        Value::Date(date) => write_date(*date),
    }
}

/// An expression as Calc writes it, in parentheses where its operator
/// binds looser than `context`.
fn write_expr(expr: &Expr, context: u8, modes: &Modes) -> String {
    let wrap = |power: u8, text: String| if power < context { format!("({text})") } else { text };
    match expr {
        Expr::Name(name) => name.clone(),
        Expr::Number(number) => {
            let text = write_number(*number, modes.float_format);
            if text.starts_with('-') && context > 0 {
                format!("({text})")
            } else {
                text
            }
        }
        Expr::Sum(terms) => {
            let mut out = String::new();
            for (index, (sign, term)) in terms.iter().enumerate() {
                let text = write_expr(term, 181, modes);
                match (index, sign) {
                    (0, true) => out.push_str(&text),
                    (0, false) => {
                        let _ = write!(out, "-{text}");
                    }
                    (_, true) => {
                        let _ = write!(out, " + {text}");
                    }
                    (_, false) => {
                        let _ = write!(out, " - {text}");
                    }
                }
            }
            wrap(180, out)
        }
        Expr::Product(a, b) => wrap(196, format!("{} {}", write_expr(a, 196, modes), write_expr(b, 197, modes))),
        Expr::Quotient(a, b) => wrap(190, format!("{} / {}", write_expr(a, 190, modes), write_expr(b, 191, modes))),
        Expr::Power(a, b) => wrap(200, format!("{}^{}", write_expr(a, 201, modes), write_expr(b, 200, modes))),
        Expr::Negative(inner) => wrap(197, format!("-{}", write_expr(inner, 197, modes))),
        Expr::Ratio(a, b) => wrap(195, format!("{a}/{b}")),
        Expr::Date(date) => write_date(*date),
        Expr::Call(name, arguments) if matches!(name.as_str(), "%" | "\\") && arguments.len() == 2 => wrap(
            190,
            format!("{} {name} {}", write_expr(&arguments[0], 190, modes), write_expr(&arguments[1], 191, modes)),
        ),
        Expr::Call(name, arguments) if name == "vector" => {
            format!(
                "[{}]",
                arguments.iter().map(|argument| write_expr(argument, 0, modes)).collect::<Vec<_>>().join(", ")
            )
        }
        Expr::Call(name, arguments) => {
            format!(
                "{name}({})",
                arguments.iter().map(|argument| write_expr(argument, 0, modes)).collect::<Vec<_>>().join(", ")
            )
        }
    }
}

/// A date form as Calc writes it with Org's `calc-date-format`,
/// `(YYYY "-" MM "-" DD " " Www (" " hh ":" mm))`: the time when the date
/// has one.
fn write_date(date: Number) -> String {
    let (day, seconds) = match date {
        Number::Int(day) => (day as i64, None),
        Number::Float(value) => {
            let day = value.floor();
            let seconds = ((value - day) * 86400.0).round() as i64;
            if seconds >= 86400 {
                (day as i64 + 1, Some(seconds - 86400))
            } else {
                (day as i64, Some(seconds))
            }
        }
    };
    let name = crate::date::WEEKDAY_ABBREVIATIONS[crate::date::weekday(day)];
    let date = crate::date::Date::from_day(day);
    match seconds {
        Some(seconds) => format!("<{date} {name} {:02}:{:02}>", seconds / 3600, seconds % 3600 / 60),
        None => format!("<{date} {name}>"),
    }
}

/// A number as Calc writes it.
pub fn write_number(number: Number, format: FloatFormat) -> String {
    match number {
        Number::Int(n) => n.to_string(),
        Number::Float(f) => write_float(f, format),
    }
}

fn write_float(value: f64, format: FloatFormat) -> String {
    if value.is_nan() {
        return "nan".to_string();
    }
    if value.is_infinite() {
        return if value > 0.0 { "inf" } else { "-inf" }.to_string();
    }
    let sign = if value < 0.0 { "-" } else { "" };
    let magnitude = value.abs();
    if magnitude == 0.0 {
        return "0.".to_string();
    }
    match format {
        FloatFormat::Fix(decimals) => format!("{sign}{magnitude:.decimals$}"),
        FloatFormat::Float(digits) | FloatFormat::Sci(digits) => {
            let digits = if digits == 0 { 12 } else { digits };
            // `d.ddde±N` to the digits shown, trailing zeros dropped.
            let scientific = format!("{magnitude:.*e}", digits - 1);
            let (mantissa, exponent) = scientific.split_once('e').unwrap_or((&scientific, "0"));
            let exponent: i64 = exponent.parse().unwrap_or(0);
            let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
            let digits = digits.trim_end_matches('0');
            let digits = if digits.is_empty() { "0" } else { digits };
            // Where the point goes, from the first digit.
            let point = exponent + 1;
            let sci = matches!(format, FloatFormat::Sci(_)) || !(-1..=12).contains(&point);
            if sci {
                let (first, rest) = digits.split_at(1);
                let rest = if rest.is_empty() { String::new() } else { format!(".{rest}") };
                format!("{sign}{first}{rest}e{exponent}")
            } else if point <= 0 {
                format!("{sign}0.{}{digits}", "0".repeat((-point) as usize))
            } else if point as usize >= digits.len() {
                format!("{sign}{digits}{}.", "0".repeat(point as usize - digits.len()))
            } else {
                let (whole, fraction) = digits.split_at(point as usize);
                format!("{sign}{whole}.{fraction}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn calc(formula: &str) -> String {
        eval(formula, &Modes::default()).unwrap_or_else(|error| format!("error: {error}"))
    }

    #[test]
    fn evaluates_as_calc_does() {
        for (formula, expected) in [
            ("1/3", "0.33333333"),
            ("7/2", "3.5"),
            ("6/3", "2"),
            ("2.5*2", "5."),
            ("0.1+0.2", "0.3"),
            ("1/7*1000000", "1.4285714e-7"),
            ("1e10", "10000000000."),
            ("123456789.0", "123456790."),
            ("0.000012345", "1.2345e-5"),
            ("2^-2", "0.25"),
            ("-2^2", "-4"),
            ("2^3^2", "512"),
            ("-7%3", "2"),
            ("7%-3", "-2"),
            ("3 2", "6"),
            ("2 -3", "-1"),
            ("2(3)", "6"),
            ("0.0625", "0.0625"),
            ("0.001", "1e-3"),
            ("1.5e12", "1.5e12"),
            ("123456789012.0", "123456790000."),
            ("2^100", "1267650600228229401496703205376"),
            ("vsum([1,2,3])", "6"),
            ("vmean([1,2,4])", "2.3333333"),
            ("vmedian([1,2,3,4])", "2.5"),
            ("vsdev([1,2,3,4])", "1.2909944"),
            ("sqrt(4)", "2"),
            ("sin(30)", "0.5"),
            ("sin(90)", "1"),
            ("round(2.5)", "3"),
            ("round(-2.5)", "-3"),
            ("x+2", "x + 2"),
            ("x*2", "2 x"),
            ("x-x", "0"),
            ("vsum([x,2,3])", "x + 5"),
            ("x^2", "x^2"),
            ("nan+1", "nan"),
            ("[1,2]+1", "[2, 3]"),
            ("vsum([])", "0"),
            ("if(2>1,10,20)", "10"),
        ] {
            assert_eq!(calc(formula), expected, "{formula}");
        }
    }
}
