# EVALJS Redis module

This module provides a way to evaluate JavaScript code inside of Redis. It uses the awesome QuickJS engine by Fabrice Bellard.

It is similar in functionality to [EVAL](https://redis.io/commands/eval).

It's now roughly on par with EVAL, and sometimes faster:

```bash
$ valkey-benchmark EVALJS "return 1 + 2" 0
Summary:
  throughput summary: 308641.97 requests per second
  latency summary (msec):
          avg       min       p50       p95       p99       max
        0.135     0.032     0.119     0.271     0.319     1.495

$ valkey-benchmark EVAL "return 1 + 2" 0
Summary:
  throughput summary: 287356.34 requests per second
  latency summary (msec):
          avg       min       p50       p95       p99       max
        0.121     0.016     0.119     0.215     0.239     0.599
```

Here are some examples:

```bash
$ valkey-cli EVALJS "return 1 + 2" 0
(integer) 3

$ valkey-cli EVALJS "return 'Hello JS!'" 0
"Hello JS!"

$ valkey-cli EVALJS "const fib = n => n <= 1 ? n : fib(n - 1) + fib(n - 2); return fib(10)" 0
(integer) 55

$ valkey-cli EVALJS "return [5, 4, 3, 2, 1].sort((a, b) => a - b)" 0
1) (integer) 1
2) (integer) 2
3) (integer) 3
4) (integer) 4
5) (integer) 5

$ valkey-cli EVALJS "return [KEYS[0], KEYS[1], ARGV[0], ARGV[1], ARGV[2]]" 2 key1 key2 arg1 arg2 arg3
1) "key1"
2) "key2"
3) "arg1"
4) "arg2"
5) "arg3"

$ valkey-cli EVALJS "return redis.call('SET', 'a', 42)" 0
"OK"

$ valkey-cli EVALJS "return redis.call('GET', 'a')" 0
"42"
```

You can also use it to implement more complex logic, such as a [distributed lock](https://redis.io/docs/latest/develop/clients/patterns/distributed-locks/):

```bash
# lock
$ valkey-cli EVALJS "return redis.call('SET', KEYS[0], ARGV[0], 'NX', 'PX', ARGV[1]) ? 1 : 0;" 1 my_lock abc123 30000
(integer) 1
# try to lock again, fails
$ valkey-cli EVALJS "return redis.call('GET', KEYS[0]) === ARGV[0] ? redis.call('DEL', KEYS[0]) : 0;" 1 my_lock abc123
(integer) 0
# unlock
$ valkey-cli EVALJS "return redis.call('GET', KEYS[0]) === ARGV[0] ? redis.call('DEL', KEYS[0]) : 0;" 1 my_lock abc123
(integer) 1
```

## Installation

You can build the module using cargo:

```sh
cargo build --release
```

Then you can load the module into a Redis server with:

```sh
valkey-server --loadmodule ./target/release/libredisjs.so

# or
valkey-cli MODULE LOAD ./target/release/libredisjs.so
```

## Testing

Run the tests with [bats](https://github.com/bats-core/bats-core); the suite starts a `valkey-server` with the module loaded on a random port and tears it down afterwards:

```sh
cargo build --release
bats tests
```

Set `MODULE_PATH` to point at a different build of the module (e.g. `target/debug/libredisjs.so`).
