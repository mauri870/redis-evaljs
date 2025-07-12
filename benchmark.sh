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

EVALJS_OUTPUT=$(redis-benchmark EVALJS "return 1 + 2" 0 2>/dev/null)
EVALJS_SUMMARY=$(echo "$EVALJS_OUTPUT" | tail -5)

EVAL_OUTPUT=$(redis-benchmark EVAL "return 1 + 2" 0 2>/dev/null)
EVAL_SUMMARY=$(echo "$EVAL_OUTPUT" | tail -5)

echo
echo "EVALJS Results:"
echo "$EVALJS_SUMMARY"
echo
echo "EVAL Results:"
echo "$EVAL_SUMMARY"
echo

EVALJS_RPS=$(echo "$EVALJS_SUMMARY" | grep "throughput summary" | awk '{print $3}')
EVAL_RPS=$(echo "$EVAL_SUMMARY" | grep "throughput summary" | awk '{print $3}')

if [[ -n "$EVALJS_RPS" && -n "$EVAL_RPS" ]]; then
    RATIO=$(echo "scale=2; $EVALJS_RPS / $EVAL_RPS * 100" | bc)
    echo "Are we there yet? EVALJS is ${RATIO}% of EVAL throughput"
fi
