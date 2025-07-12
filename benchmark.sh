#!/bin/bash

set -e

cleanup() {
    echo "Cleaning up..."
    if [[ -n $REDIS_PID ]]; then
        kill $REDIS_PID 2>/dev/null || true
        wait $REDIS_PID 2>/dev/null || true
    fi
    pkill redis-server 2>/dev/null || true
}

trap cleanup EXIT INT TERM

echo "Building module..."
cargo build --release >/dev/null 2>&1

pkill redis-server 2>/dev/null || true

echo "Starting Redis server..."
redis-server --loadmodule ./target/release/libredisjs.so >/dev/null 2>&1 &
REDIS_PID=$!

sleep 1

if ! kill -0 $REDIS_PID 2>/dev/null; then
    echo "Error: Redis server failed to start"
    exit 1
fi

echo "Running benchmarks..."

echo "=== Simple Math Test ==="
EVALJS_OUTPUT=$(redis-benchmark EVALJS "return 1 + 2" 0 2>/dev/null)
EVALJS_SUMMARY=$(echo "$EVALJS_OUTPUT" | tail -5)

EVAL_OUTPUT=$(redis-benchmark EVAL "return 1 + 2" 0 2>/dev/null)
EVAL_SUMMARY=$(echo "$EVAL_OUTPUT" | tail -5)

echo
echo "EVALJS Results (Math):"
echo "$EVALJS_SUMMARY"
echo
echo "EVAL Results (Math):"
echo "$EVAL_SUMMARY"
echo

EVALJS_MATH_RPS=$(echo "$EVALJS_SUMMARY" | grep "throughput summary" | awk '{print $3}')
EVAL_MATH_RPS=$(echo "$EVAL_SUMMARY" | grep "throughput summary" | awk '{print $3}')

if [[ -n "$EVALJS_MATH_RPS" && -n "$EVAL_MATH_RPS" ]]; then
    MATH_RATIO=$(echo "scale=2; $EVALJS_MATH_RPS / $EVAL_MATH_RPS * 100" | bc)
    echo "Math Test: EVALJS is ${MATH_RATIO}% of EVAL throughput"
fi

echo
echo "=== Redis Call Test ==="
EVALJS_CALL_OUTPUT=$(redis-benchmark EVALJS "return redis.call('SET', 'a', 42)" 0 2>/dev/null)
EVALJS_CALL_SUMMARY=$(echo "$EVALJS_CALL_OUTPUT" | tail -5)

EVAL_CALL_OUTPUT=$(redis-benchmark EVAL "return redis.call('SET', 'a', 42)" 0 2>/dev/null)
EVAL_CALL_SUMMARY=$(echo "$EVAL_CALL_OUTPUT" | tail -5)

echo
echo "EVALJS Results (Redis Call):"
echo "$EVALJS_CALL_SUMMARY"
echo
echo "EVAL Results (Redis Call):"
echo "$EVAL_CALL_SUMMARY"
echo

EVALJS_CALL_RPS=$(echo "$EVALJS_CALL_SUMMARY" | grep "throughput summary" | awk '{print $3}')
EVAL_CALL_RPS=$(echo "$EVAL_CALL_SUMMARY" | grep "throughput summary" | awk '{print $3}')

if [[ -n "$EVALJS_CALL_RPS" && -n "$EVAL_CALL_RPS" ]]; then
    CALL_RATIO=$(echo "scale=2; $EVALJS_CALL_RPS / $EVAL_CALL_RPS * 100" | bc)
    echo "Redis Call Test: EVALJS is ${CALL_RATIO}% of EVAL throughput"
fi

echo
echo "=== Are we there yet? ==="
if [[ -n "$EVALJS_MATH_RPS" && -n "$EVAL_MATH_RPS" ]]; then
    echo "Math: EVALJS ${EVALJS_MATH_RPS} req/s vs EVAL ${EVAL_MATH_RPS} req/s (${MATH_RATIO}%)"
fi
if [[ -n "$EVALJS_CALL_RPS" && -n "$EVAL_CALL_RPS" ]]; then
    echo "Redis Call: EVALJS ${EVALJS_CALL_RPS} req/s vs EVAL ${EVAL_CALL_RPS} req/s (${CALL_RATIO}%)"
fi
