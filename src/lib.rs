mod redisjs;
mod vm;

#[macro_use]
extern crate redis_module;

use redis_module::{
    Context, NextArg, RedisError, RedisResult, RedisString, RedisValue, Status, ThreadSafeContext,
};
use rquickjs::{Type, Value as QJSValue};
use std::{
    cell::RefCell,
    sync::{mpsc, Arc, Mutex, OnceLock},
    thread,
};

static THREAD_POOL: OnceLock<ThreadPool> = OnceLock::new();

// VM is thread-local to avoid contention
thread_local! {
    static VM: RefCell<Option<vm::VM>> = RefCell::new(None);
}

struct ThreadPool {
    sender: mpsc::Sender<Job>,
}

type Job = Box<dyn FnOnce() + Send + 'static>;

impl ThreadPool {
    fn new(size: usize) -> ThreadPool {
        let (sender, receiver) = mpsc::channel::<Job>();
        let receiver = Arc::new(Mutex::new(receiver));

        for _ in 0..size {
            let receiver = Arc::clone(&receiver);
            thread::spawn(move || loop {
                match receiver.lock().unwrap().recv() {
                    Ok(job) => job(),
                    Err(_) => break,
                }
            });
        }

        ThreadPool { sender }
    }

    fn execute<F>(&self, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        let job = Box::new(f);
        self.sender.send(job).unwrap();
    }
}

fn init(_ctx: &Context, _args: &[RedisString]) -> Status {
    // Initialize thread pool
    THREAD_POOL.get_or_init(|| ThreadPool::new(num_cpus::get()));

    Status::Ok
}

fn evaljs_cmd(ctx: &Context, args: Vec<RedisString>) -> RedisResult {
    if args.len() < 3 {
        return Err(RedisError::WrongArity);
    }

    let mut args = args.into_iter().skip(1);

    let code = match args.next() {
        Some(v) => v.to_string(),
        None => return Err(RedisError::WrongArity),
    };

    let numkeys = args
        .next_u64()
        .map_err(|_| RedisError::Str("ERR invalid number of keys"))? as usize;

    let keys: Vec<String> = args.by_ref().take(numkeys).map(Into::into).collect();
    let argv: Vec<String> = args.map(Into::into).collect();

    let blocked_client = ctx.block_client();

    let pool = THREAD_POOL.get().expect("Thread pool not initialized");
    pool.execute(move || {
        let thread_ctx = ThreadSafeContext::with_blocked_client(blocked_client);

        // Get or create thread-local VM
        let vm_result = VM.with(|vm_cell| {
            let mut vm_opt = vm_cell.borrow_mut();
            if vm_opt.is_none() {
                *vm_opt = Some(vm::VM::new().expect("failed to initialize QJS VM"));
            }
            vm_opt.as_ref().unwrap().eval(|ctx| {
                let globals = ctx.globals();
                globals.set("KEYS", &keys).expect("failed to set KEYS");
                globals.set("ARGV", &argv).expect("failed to set ARGV");

                let mut wrapper = String::with_capacity(code.len() + 30);
                wrapper.push_str("(function(){");
                wrapper.push_str(&code);
                wrapper.push_str("})();");

                match ctx.eval(wrapper) {
                    Ok(v) => RedisResult::Ok(Value(v).into()),
                    Err(e) => RedisResult::Err(RedisError::String(e.to_string())),
                }
            })
        });

        thread_ctx.reply(vm_result);
    });
    Ok(RedisValue::NoReply)
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
            Type::Null | Type::Uninitialized | Type::Undefined | Type::Unknown => RedisValue::Null,
            Type::Array => {
                let arr = v
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| Value(v.unwrap()).into())
                    .collect();
                RedisValue::Array(arr)
            }
            _ => RedisValue::StaticError("unsupported type"),
        }
    }
}

//////////////////////////////////////////////////////

redis_module! {
    name: "evaljs",
    version: 1,
    allocator: (redis_module::alloc::RedisAlloc, redis_module::alloc::RedisAlloc),
    data_types: [],
    init: init,
    commands: [
        ["EVALJS", evaljs_cmd, "", 0, 0, 0],
    ],
}
