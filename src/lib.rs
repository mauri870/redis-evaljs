use redis_module::{redis_module, Context, RedisError, RedisResult, RedisString, RedisValue};
use rquickjs::{Context as QJSContext, Runtime, Type, Value as QJSValue};

fn evaljs_cmd(_ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    if args.len() < 3 {
        return Err(RedisError::WrongArity);
    }

    let rt = Runtime::new()?;
    rt.set_max_stack_size(64 * 1024);
    let ctx = QJSContext::full(&rt)?;

    let mut args = args.into_iter().skip(1);

    let code = args.next().unwrap().clone();
    let envelope = format!("(function() {{ {} }})()", code);
    let mut result: RedisResult = RedisResult::Ok(RedisValue::Null);
    ctx.with(|ctx| {
        result = match ctx.eval(envelope) {
            Ok(v) => RedisResult::Ok(Value(v).into()),
            Err(e) => RedisResult::Err(RedisError::String(e.to_string())),
        };
    });

    result
}

struct Value<'a>(QJSValue<'a>);

impl From<Value<'_>> for RedisValue {
    fn from(val: Value) -> Self {
        let v = val.0;
        match v.type_of() {
            Type::Bool => RedisValue::Bool(v.as_bool().unwrap()),
            Type::Int => RedisValue::Integer(v.as_int().unwrap() as i64),
            Type::Float => RedisValue::Float(v.as_float().unwrap()),
            Type::String => RedisValue::BulkString(unsafe { v.ref_string() }.to_string().unwrap()),
            Type::Null | Type::Undefined | Type::Unknown => RedisValue::Null,
            _ => RedisValue::StaticError("unsupported type"),
            // Symbol | Object | Array | Function | Constructor => {
            //     write!(f, "(")?;
            //     unsafe { self.get_ptr() }.fmt(f)?;
            //     write!(f, ")")?;
            // }
            // Exception => {
            //     writeln!(f, "(")?;
            //     self.as_exception().unwrap().fmt(f)?;
            //     writeln!(f, ")")?;
            // }
            // Uninitialized => "uninitialized".fmt(f)?,
            // Module => "module".fmt(f)?,
            // BigInt => "BigInt".fmt(f)?,
        }
    }
}

//////////////////////////////////////////////////////

redis_module! {
    name: "evaljs",
    version: 1,
    allocator: (redis_module::alloc::RedisAlloc, redis_module::alloc::RedisAlloc),
    data_types: [],
    commands: [
        ["EVALJS", evaljs_cmd, "", 0, 0, 0],
    ],
}
