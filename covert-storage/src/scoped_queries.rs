use std::{collections::HashSet, fmt::Display};

use sqlparser::{
    ast::{
        ColumnOption, Expr, Ident, ObjectName, Query, ReferentialAction, Select, SelectItem,
        SetExpr, Statement, TableConstraint, TableFactor, TableWithJoins,
    },
    dialect::SQLiteDialect,
    parser::{Parser, ParserError},
};

const SYSTEM_TABLE_REFERENCE: &str = "__SYSTEM__";

#[derive(Debug, PartialEq)]
pub enum Error {
    TableRenameNotSupported,
    UnsupportedStatement(Statement),
    Parser(ParserError),
    SystemTableRestrict,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ScopedQuery {
    pub sql: String,
}

impl ScopedQuery {
    pub fn new(table_prefix: &str, sql: &str) -> Result<Self, Error> {
        Ok(Self {
            sql: prefix_query(table_prefix, sql)?,
        })
    }

    pub fn sql(&self) -> &str {
        &self.sql
    }
}

#[derive(Debug)]
struct TableAndAliases {
    table_or_alias: HashSet<String>,
}

impl TableAndAliases {
    pub fn add_table_or_alias<T: Display>(&mut self, table: &T) {
        self.table_or_alias.insert(table.to_string());
    }
}

fn prefix_query(table_prefix: &str, sql: &str) -> Result<String, Error> {
    let mut tables = TableAndAliases {
        table_or_alias: HashSet::default(),
    };
    let statements = Parser::parse_sql(&SQLiteDialect {}, sql).map_err(Error::Parser)?;

    let mut scoped_sql = vec![];

    for mut stmt in statements {
        get_tables_in_statement(&mut tables, &mut stmt, table_prefix)?;

        // Too much hassle to traverse AST to map projections so this hacky
        // should also be fine.
        let scoped_stmt = stmt
            .to_string()
            .split_ascii_whitespace()
            .map(|item| {
                for table in &tables.table_or_alias {
                    let refed = format!("{}.", &table[table_prefix.len()..]);
                    if item.starts_with(&refed) {
                        return item.replace(&refed, &format!("{table}."));
                    }
                }

                item.to_string()
            })
            .collect::<Vec<_>>()
            .join(" ");
        scoped_sql.push(scoped_stmt);
    }

    let scoped_sql = format!("{};", scoped_sql.join(";"));

    Ok(scoped_sql)
}

fn handle_fk_table_name_prefix(
    prefix: &str,
    foreign_table: &mut ObjectName,
    on_delete: &mut Option<ReferentialAction>,
    on_update: &mut Option<ReferentialAction>,
) -> Result<(), Error> {
    let foreign_table_name = foreign_table.to_string();
    if foreign_table_name.starts_with(SYSTEM_TABLE_REFERENCE) {
        *foreign_table = ObjectName(vec![Ident {
            // Empty prefix on purpose
            value: foreign_table_name.replacen(SYSTEM_TABLE_REFERENCE, "", 1),
            quote_style: None,
        }]);
        if matches!(on_delete, Some(ReferentialAction::Restrict))
            || matches!(on_update, Some(ReferentialAction::Restrict))
        {
            return Err(Error::SystemTableRestrict);
        }
    } else {
        *foreign_table = ObjectName(vec![Ident {
            value: format!("{prefix}{foreign_table_name}"),
            quote_style: None,
        }]);
    }
    Ok(())
}

// TODO: this needs a lot of work
fn get_tables_in_expr(
    tables: &mut TableAndAliases,
    expr: &mut Expr,
    prefix: &str,
) -> Result<(), Error> {
    // eprintln!("Expr is: {expr:#?}");
    match expr {
        Expr::Subquery(query) | Expr::ArraySubquery(query) => {
            get_tables_in_stmt_query(tables, query, prefix)?;
        }
        Expr::AllOp(expr) => get_tables_in_expr(tables, expr, prefix)?,
        Expr::BinaryOp { left, right, .. } => {
            get_tables_in_expr(tables, left, prefix)?;
            get_tables_in_expr(tables, right, prefix)?;
        }
        _ => {
            // TODO: make it not supported
        }
    }
    Ok(())
}

fn get_tables_in_set_expr(
    tables: &mut TableAndAliases,
    expr: &mut SetExpr,
    prefix: &str,
) -> Result<(), Error> {
    match expr {
        SetExpr::Table(table) => {
            if let Some(table_name) = table.table_name.as_mut() {
                *table_name = format!("{prefix}{table_name}");
                tables.add_table_or_alias(table_name);
            }
        }
        SetExpr::Select(select) => {
            let Select {
                from,
                into,
                projection,
                selection,
                ..
            } = select.as_mut();

            for proj in projection {
                match proj {
                    SelectItem::UnnamedExpr(expr) => get_tables_in_expr(tables, expr, prefix)?,
                    SelectItem::ExprWithAlias { expr, .. } => {
                        get_tables_in_expr(tables, expr, prefix)?;
                    }
                    _ => (),
                }
            }

            if let Some(selection) = selection {
                get_tables_in_expr(tables, selection, prefix)?;
            }

            for table in from {
                get_tables_in_table_with_join_expr(tables, table, prefix)?;
            }
            if let Some(into) = into.as_mut() {
                into.name = ObjectName(vec![Ident {
                    value: format!("{prefix}{}", into.name),
                    quote_style: None,
                }]);
                tables.add_table_or_alias(&into.name);
            }
        }
        SetExpr::Insert(insert) => {
            get_tables_in_statement(tables, insert, prefix)?;
        }
        SetExpr::SetOperation { left, right, .. } => {
            get_tables_in_set_expr(tables, left, prefix)?;
            get_tables_in_set_expr(tables, right, prefix)?;
        }
        SetExpr::Query(query) => {
            get_tables_in_stmt_query(tables, query, prefix)?;
        }
        SetExpr::Values(_) => {}
    };
    Ok(())
}

fn get_tables_in_stmt_query(
    tables: &mut TableAndAliases,
    expr: &mut Query,
    prefix: &str,
) -> Result<(), Error> {
    get_tables_in_set_expr(tables, &mut expr.body, prefix)?;

    if let Some(with) = expr.with.as_mut() {
        for cte in &mut with.cte_tables {
            get_tables_in_stmt_query(tables, &mut cte.query, prefix)?;
            if let Some(from) = cte.from.as_mut() {
                *from = Ident {
                    value: format!("{prefix}{from}"),
                    quote_style: None,
                };
                tables.add_table_or_alias(from);
            }
            cte.alias.name = Ident {
                value: format!("{prefix}{}", &cte.alias.name),
                quote_style: None,
            };
            tables.add_table_or_alias(&cte.alias.name);
        }
    }
    Ok(())
}

fn get_tables_in_table_factor(
    tables: &mut TableAndAliases,
    expr: &mut TableFactor,
    prefix: &str,
) -> Result<(), Error> {
    match expr {
        sqlparser::ast::TableFactor::Table { name, alias, .. } => {
            if let Some(alias) = alias.as_mut() {
                alias.name = Ident {
                    value: format!("{prefix}{}", &alias.name),
                    quote_style: None,
                };
                tables.add_table_or_alias(&alias.name);
            }
            for ident in &mut name.0 {
                *ident = Ident {
                    value: format!("{prefix}{ident}"),
                    quote_style: None,
                };
                tables.add_table_or_alias(ident);
            }
        }
        sqlparser::ast::TableFactor::Derived {
            subquery, alias, ..
        } => {
            if let Some(alias) = alias.as_mut() {
                alias.name = Ident {
                    value: format!("{prefix}{}", &alias.name),
                    quote_style: None,
                };
                tables.add_table_or_alias(&alias.name);
            }
            get_tables_in_stmt_query(tables, subquery, prefix)?;
        }
        sqlparser::ast::TableFactor::TableFunction { alias, expr, .. } => {
            if let Some(alias) = alias.as_mut() {
                alias.name = Ident {
                    value: format!("{prefix}{}", &alias.name),
                    quote_style: None,
                };
                tables.add_table_or_alias(&alias.name);
            }
            get_tables_in_expr(tables, expr, prefix)?;
        }
        sqlparser::ast::TableFactor::UNNEST { alias, .. } => {
            if let Some(alias) = alias.as_mut() {
                alias.name = Ident {
                    value: format!("{prefix}{}", &alias.name),
                    quote_style: None,
                };
                tables.add_table_or_alias(&alias.name);
            }
        }
        sqlparser::ast::TableFactor::NestedJoin {
            table_with_joins,
            alias,
        } => {
            if let Some(alias) = alias.as_mut() {
                alias.name = Ident {
                    value: format!("{prefix}{}", &alias.name),
                    quote_style: None,
                };
                tables.add_table_or_alias(&alias.name);
            }
            get_tables_in_table_with_join_expr(tables, table_with_joins, prefix)?;
        }
    }

    Ok(())
}

fn get_tables_in_table_with_join_expr(
    tables: &mut TableAndAliases,
    expr: &mut TableWithJoins,
    prefix: &str,
) -> Result<(), Error> {
    get_tables_in_table_factor(tables, &mut expr.relation, prefix)?;
    for join in &mut expr.joins {
        get_tables_in_table_factor(tables, &mut join.relation, prefix)?;
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn get_tables_in_statement(
    tables: &mut TableAndAliases,
    statement: &mut Statement,
    prefix: &str,
) -> Result<(), Error> {
    match statement {
        Statement::Query(query) => {
            get_tables_in_stmt_query(tables, query, prefix)?;
        }
        Statement::Insert {
            table_name, source, ..
        } => {
            for ident in &mut table_name.0 {
                *ident = Ident {
                    value: format!("{prefix}{ident}"),
                    quote_style: None,
                };
                tables.add_table_or_alias(ident);
            }
            get_tables_in_stmt_query(tables, source, prefix)?;
        }
        Statement::Update { from, table, .. } => {
            if let Some(from) = from {
                get_tables_in_table_with_join_expr(tables, from, prefix)?;
            }
            get_tables_in_table_with_join_expr(tables, table, prefix)?;
        }
        Statement::StartTransaction { .. }
        | Statement::Commit { .. }
        | Statement::Comment { .. } => {}
        Statement::CreateTable {
            name,
            query,
            columns,
            constraints,
            ..
        } => {
            *name = ObjectName(vec![Ident {
                value: format!("{prefix}{name}"),
                quote_style: None,
            }]);
            tables.add_table_or_alias(name);
            if let Some(query) = query.as_mut() {
                get_tables_in_stmt_query(tables, query, prefix)?;
            }

            for constraint in constraints {
                if let TableConstraint::ForeignKey {
                    foreign_table,
                    on_delete,
                    on_update,
                    ..
                } = constraint
                {
                    handle_fk_table_name_prefix(prefix, foreign_table, on_delete, on_update)?;
                }
            }
            for column in columns {
                for option in &mut column.options {
                    if let ColumnOption::ForeignKey {
                        foreign_table,
                        on_delete,
                        on_update,
                        ..
                    } = &mut option.option
                    {
                        handle_fk_table_name_prefix(prefix, foreign_table, on_delete, on_update)?;
                    }
                }
            }
        }
        Statement::Drop { names, .. } => {
            for name in names {
                *name = ObjectName(vec![Ident {
                    value: format!("{prefix}{name}"),
                    quote_style: None,
                }]);
                tables.add_table_or_alias(name);
            }
        }
        Statement::AlterTable { name, operation } => {
            if let sqlparser::ast::AlterTableOperation::RenameTable { .. } = operation {
                return Err(Error::TableRenameNotSupported);
            };
            *name = ObjectName(vec![Ident {
                value: format!("{prefix}{name}"),
                quote_style: None,
            }]);
            tables.add_table_or_alias(name);
        }
        Statement::CreateView { name, query, .. } => {
            *name = ObjectName(vec![Ident {
                value: format!("{prefix}{name}"),
                quote_style: None,
            }]);
            tables.add_table_or_alias(name);
            get_tables_in_stmt_query(tables, query, prefix)?;
        }
        Statement::Delete {
            table_name,
            selection,
            ..
        } => {
            get_tables_in_table_factor(tables, table_name, prefix)?;
            if let Some(selection) = selection {
                get_tables_in_expr(tables, selection, prefix)?;
            }
        }
        Statement::CreateIndex {
            name, table_name, ..
        } => {
            *name = ObjectName(vec![Ident {
                value: format!("{prefix}{name}"),
                quote_style: None,
            }]);
            tables.add_table_or_alias(name);

            *table_name = ObjectName(vec![Ident {
                value: format!("{prefix}{table_name}"),
                quote_style: None,
            }]);
            tables.add_table_or_alias(table_name);
        }
        stmt => {
            return Err(Error::UnsupportedStatement(stmt.clone()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::*;

    #[derive(Debug, Deserialize)]
    struct TestCase {
        prefix: String,
        input: String,
        output: String,
        #[serde(default)]
        skip: bool,
    }

    #[test]
    fn scoped_sql() {
        let paths = std::fs::read_dir("./fixtures/scoped_sql/").unwrap();
        for path in paths {
            let file = std::fs::read(path.unwrap().path()).unwrap();
            let test: TestCase = serde_json::from_slice(&file).unwrap();
            if test.skip {
                eprintln!("skipping test case: {:#?}", test.input);
                continue;
            }

            let query = ScopedQuery::new(&test.prefix, &test.input).unwrap();
            assert_eq!(query.sql(), test.output);
        }
    }

    #[test]
    fn table_rename_not_supported() {
        let sql = "ALTER TABLE users RENAME TO old_users";
        let err = ScopedQuery::new("foo_", sql.into()).unwrap_err();
        assert_eq!(err, Error::TableRenameNotSupported);
    }

    #[test]
    fn unsupported_analyze_statement() {
        let sql = "ANALYZE TABLE users";
        let err = ScopedQuery::new("foo_", sql.into()).unwrap_err();
        assert_eq!(
            err,
            Error::UnsupportedStatement(Statement::Analyze {
                table_name: ObjectName(vec![Ident {
                    value: "users".into(),
                    quote_style: None
                }]),
                partitions: Default::default(),
                for_columns: Default::default(),
                columns: Default::default(),
                cache_metadata: Default::default(),
                noscan: Default::default(),
                compute_statistics: Default::default()
            })
        );
    }

    #[test]
    fn multiple_statements() {
        let sql = r#"
START TRANSACTION;

UPDATE accounts
   SET balance = balance - 1000
 WHERE account_no = 100;

UPDATE accounts
   SET balance = balance + 1000
 WHERE account_no = 200;
 
INSERT INTO account_changes(account_no,flag,amount,changed_at) 
VALUES(100,'-',1000,datetime('now'));

INSERT INTO account_changes(account_no,flag,amount,changed_at) 
VALUES(200,'+',1000,datetime('now'));

COMMIT;
        "#;
        let expected_scoped_sql = r#"START TRANSACTION;UPDATE foo_accounts SET balance = balance - 1000 WHERE account_no = 100;UPDATE foo_accounts SET balance = balance + 1000 WHERE account_no = 200;INSERT INTO foo_account_changes (account_no, flag, amount, changed_at) VALUES (100, '-', 1000, datetime('now'));INSERT INTO foo_account_changes (account_no, flag, amount, changed_at) VALUES (200, '+', 1000, datetime('now'));COMMIT;"#;
        let scoped_sql = ScopedQuery::new("foo_", sql.into()).unwrap();
        assert_eq!(scoped_sql.sql(), expected_scoped_sql);
    }

    #[test]
    fn pragma_not_working() {
        let sql = "PRAGMA rekey = 'newkey'";
        assert!(ScopedQuery::new("foo_", sql.into()).is_err());
    }

    #[test]
    fn fk_with_restriction_on_system_table_is_rejected() {
        let tests = [
            "CREATE TABLE IF NOT EXISTS users (
                id INTEGER, 
                policy_id INTEGER REFERENCES __SYSTEM__policies(id) ON DELETE RESTRICT
            )",
            "CREATE TABLE IF NOT EXISTS users (
                id INTEGER, 
                policy_id INTEGER REFERENCES __SYSTEM__policies(id) ON UPDATE RESTRICT
            )",
            "CREATE TABLE IF NOT EXISTS users (
                id INTEGER, 
                policy_id INTEGER, 
                FOREGIN KEY (policy_id) REFERENCES __SYSTEM__policies(id) ON UPDATE RESTRICT
            )",
        ];
        for test in tests {
            assert_eq!(
                ScopedQuery::new("foo_", test.into()),
                Err(Error::SystemTableRestrict)
            );
        }
    }
}
