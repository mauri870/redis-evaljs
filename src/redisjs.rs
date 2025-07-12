use rquickjs::{
    prelude::{Func, Rest},
    Ctx, IntoJs, Object, Result, Value,
};

pub fn init(ctx: &Ctx<'_>) -> Result<()> {
    let globals = ctx.globals();

    let redis = Object::new(ctx.clone())?;

    redis.set("call", Func::from(call))?;

    globals.set("redis", redis)?;

    Ok(())
}

fn call<'js>(ctx: Ctx<'js>, args: Rest<Value<'js>>) -> Result<Value<'js>> {
    let strargs: Vec<String> = args
        .iter()
        .map(|v| unsafe { v.ref_string() }.to_string().unwrap())
        .collect();

    // Create string slice references more efficiently
    let cmdargs: Vec<&str> = strargs.iter().map(|s| s.as_str()).collect();

    let rctx = redis_module::MODULE_CONTEXT.lock();
    let res = rctx
        .call(cmdargs[0], &cmdargs[1..])
        .expect("failed to call redis");

    // Handle more Redis return types
    match res {
        redis_module::RedisValue::SimpleString(s) => s.into_js(&ctx),
        redis_module::RedisValue::BulkString(s) => s.into_js(&ctx),
        redis_module::RedisValue::Integer(i) => i.into_js(&ctx),
        redis_module::RedisValue::Float(f) => f.into_js(&ctx),
        redis_module::RedisValue::Bool(b) => b.into_js(&ctx),
        redis_module::RedisValue::Array(arr) => {
            let js_arr = rquickjs::Array::new(ctx.clone())?;
            for (i, item) in arr.iter().enumerate() {
                match item {
                    redis_module::RedisValue::SimpleString(s) => js_arr.set(i, s)?,
                    redis_module::RedisValue::BulkString(s) => js_arr.set(i, s)?,
                    redis_module::RedisValue::Integer(n) => js_arr.set(i, *n)?,
                    redis_module::RedisValue::Float(f) => js_arr.set(i, *f)?,
                    redis_module::RedisValue::Bool(b) => js_arr.set(i, *b)?,
                    _ => js_arr.set(i, Value::new_null(ctx.clone()))?,
                }
            }
            Ok(js_arr.into_value())
        }
        redis_module::RedisValue::Null => Ok(Value::new_null(ctx)),
        _ => Ok(Value::new_null(ctx)),
    }
}
