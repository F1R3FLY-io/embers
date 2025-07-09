use std::collections::HashMap;

use anyhow::{Context, anyhow};

use crate::models::rhoapi::expr::ExprInstance;

pub trait FromExpr: Sized {
    fn from(val: ExprInstance) -> anyhow::Result<Self>;
}

impl FromExpr for String {
    fn from(val: ExprInstance) -> anyhow::Result<Self> {
        match val {
            ExprInstance::GString(value) => Ok(value),
            other => Err(anyhow!("unexpected expr type: {other:?} expected GString")),
        }
    }
}

impl<T> FromExpr for Vec<T>
where
    T: FromExpr,
{
    fn from(val: ExprInstance) -> anyhow::Result<Self> {
        match val {
            ExprInstance::EListBody(list) => list
                .ps
                .into_iter()
                .map(|par| {
                    let expr = par.exprs.into_iter().next().context("missing exprs")?;
                    let expr = expr.expr_instance.context("missing expr_instance")?;
                    T::from(expr)
                })
                .collect(),
            other => Err(anyhow!(
                "unexpected expr type: {other:?} expected EListBody"
            )),
        }
    }
}

impl<T> FromExpr for HashMap<String, T>
where
    T: FromExpr,
{
    fn from(val: ExprInstance) -> anyhow::Result<Self> {
        match val {
            ExprInstance::EMapBody(map) => map
                .kvs
                .into_iter()
                .map(|pair| {
                    let key = pair
                        .key
                        .and_then(|key| key.exprs.into_iter().next())
                        .and_then(|expr| expr.expr_instance)
                        .context("missing key")?;
                    let key = FromExpr::from(key)?;

                    let value = pair
                        .value
                        .and_then(|value| value.exprs.into_iter().next())
                        .and_then(|expr| expr.expr_instance)
                        .context("missing value")?;
                    let value = FromExpr::from(value)?;

                    Ok((key, value))
                })
                .collect(),
            other => Err(anyhow!("unexpected expr type: {other:?} expected EMapBody")),
        }
    }
}

impl FromExpr for Vec<u8> {
    fn from(val: ExprInstance) -> anyhow::Result<Self> {
        match val {
            ExprInstance::GByteArray(list) => Ok(list),
            other => Err(anyhow!(
                "unexpected expr type: {other:?} expected GByteArray"
            )),
        }
    }
}

pub trait ShortHex {
    fn short_hex(&self, length: usize) -> String;
}

impl<T> ShortHex for T
where
    T: AsRef<[u8]>,
{
    fn short_hex(&self, length: usize) -> String {
        let slice = self.as_ref();
        if slice.len() > length {
            format!("{}...", hex::encode(&slice[..length]))
        } else {
            hex::encode(slice)
        }
    }
}
