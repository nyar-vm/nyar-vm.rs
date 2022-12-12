//! Mini Rust 抽象语法树定义

use serde::{Deserialize, Serialize};

/// 程序主结构
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Program {
    pub name: String,
    pub functions: Vec<Function>,
    pub structs: Vec<StructDefinition>,
}

/// 结构体定义
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructDefinition {
    pub name: String,
    pub fields: Vec<FieldDefinition>,
}

/// 字段定义
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldDefinition {
    pub name: String,
    pub field_type: Type,
}

/// 函数定义
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Function {
    pub name: String,
    pub parameters: Vec<Parameter>,
    pub return_type: Option<Type>,
    pub body: Block,
}

/// 函数参数
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Parameter {
    pub name: String,
    pub param_type: Type,
    pub is_mutable: bool,
}

/// 代码块
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Block {
    pub statements: Vec<Statement>,
}

/// 语句
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Statement {
    /// 表达式语句
    Expression(Expression),
    /// 变量声明
    VariableDeclaration {
        name: String,
        var_type: Option<Type>,
        initializer: Option<Expression>,
        is_mutable: bool,
    },
    /// 返回语句
    Return(Option<Expression>),
    /// 条件语句
    If {
        condition: Expression,
        then_branch: Block,
        else_branch: Option<Block>,
    },
    /// 循环语句
    While { condition: Expression, body: Block },
    /// For 循环语句
    For {
        var_name: String,
        iterable: Expression,
        body: Block,
    },
    /// 赋值语句
    Assignment {
        target: Expression,
        value: Expression,
    },
}

/// 表达式
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Expression {
    /// 字面量
    Literal(Literal),
    /// 标识符
    Identifier(String),
    /// 范围表达式: start..end
    Range {
        start: Box<Expression>,
        end: Box<Expression>,
    },
    /// 结构体实例化
    StructInstantiation {
        name: String,
        fields: Vec<(String, Expression)>,
    },
    /// 字段访问: obj.field
    FieldAccess {
        object: Box<Expression>,
        field: String,
    },
    /// 数组字面量: [1, 2, 3]
    ArrayLiteral(Vec<Expression>),
    /// 索引访问: arr[index]
    IndexAccess {
        object: Box<Expression>,
        index: Box<Expression>,
    },
    /// 函数调用
    FunctionCall {
        name: String,
        arguments: Vec<Expression>,
    },
    /// 宏调用（如 println!）
    MacroCall {
        name: String,
        arguments: Vec<Expression>,
    },
    /// 方法调用 (如 console.log)
    MethodCall {
        object: Box<Expression>,
        method: String,
        arguments: Vec<Expression>,
    },
    /// 二元运算
    BinaryOperation {
        left: Box<Expression>,
        operator: BinaryOperator,
        right: Box<Expression>,
    },
    /// 一元运算
    UnaryOperation {
        operator: UnaryOperator,
        operand: Box<Expression>,
    },
}

/// 字面量
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Literal {
    /// 整数
    Integer(i64),
    /// 浮点数
    Float(f64),
    /// 字符串
    String(String),
    /// 字符
    Char(char),
    /// 布尔值
    Boolean(bool),
}

/// 二元运算符
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    And,
    Or,
}

/// 一元运算符
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum UnaryOperator {
    Negate,
    Not,
}

/// 类型
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Type {
    /// 32位整数
    I32,
    /// 64位整数
    I64,
    /// 32位浮点数
    F32,
    /// 64位浮点数
    F64,
    /// 字符串
    String,
    /// 布尔值
    Bool,
    /// 字符类型
    Char,
    /// 空类型
    Unit,
    /// 自定义类型 (结构体)
    Custom(String),
    /// 数组类型: [T; N] 或 [T] (目前简单实现为 [T])
    Array(Box<Type>),
}
