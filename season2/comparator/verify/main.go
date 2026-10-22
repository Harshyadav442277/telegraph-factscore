// Command verify runs a comparator .wasm through the node's own runtime
// (telegraph-wasm-baseline pkg/wasm/runtime.NewDeterministicInterruptible) and checks
// every result against the network's answer key (pkg/scoring/comparator.ReferenceScore).
//
// The case grid follows the published gate: every code and some unknown ones, several
// tolerances including 0 and negative, truths from 1 to wei scale and negative, and miner
// values at an exact match, one unit off, the half-score point, the window edge and past it,
// on both sides. A seeded random sweep is added, and the whole run is repeated to check that
// the module is deterministic. It passes only on zero mismatches. The gate allows ±10 bp;
// this check allows none.
//
//	go run . <module.wasm>
//	go run . pack <base.wasm> <out-dir> INTENT,...   (see pack.go)
package main

import (
	"context"
	"fmt"
	"math"
	"math/rand"
	"os"

	"github.com/telegraphprotocol/telegraph-wasm-baseline/pkg/scoring/comparator"
	"github.com/telegraphprotocol/telegraph-wasm-baseline/pkg/wasm/runtime"
)

type tc struct{ code, tol, truth, miner int64 }

func grid() []tc {
	codes := []int64{0, 1, 2, 3, 4, -1, 99, 1<<32 + 1, 1<<32 + 2}
	tols := []int64{0, -5, 1, 7, 50, 100, 500, 2500, 10000, 1_000_000_000, math.MaxInt64}
	truths := []int64{1, 2, 7, 100, 12_345, 1_000_000, 1e9, 1e12, 1e15, 1e18, math.MaxInt64 / 2}
	var out []tc
	for _, c := range codes {
		for _, t := range tols {
			for _, base := range truths {
				for _, truth := range []int64{base, -base} {
					// offsets in absolute units and, for RelTol, in basis points of |truth|
					rel := func(bps int64) int64 {
						v := float64(truth) * float64(bps) / 10000
						if math.Abs(v) > 9e18 {
							return 0
						}
						return int64(v)
					}
					offs := []int64{0, 1, -1, t, -t, 2*t - 1, -(2*t - 1), 2 * t, -2 * t, 2*t + 1,
						rel(t), -rel(t), rel(2*t), -rel(2*t), rel(2*t) - 1, rel(2*t) + 1}
					for _, o := range offs {
						out = append(out, tc{c, t, truth, truth + o})
					}
				}
			}
		}
	}
	r := rand.New(rand.NewSource(20261022))
	for i := 0; i < 200_000; i++ {
		truth := r.Int63n(1e18) + 1
		if r.Intn(4) == 0 {
			truth = -truth
		}
		scale := []int64{10, 1000, 1e6, 1e12, 1e18}[r.Intn(5)]
		miner := truth + r.Int63n(scale) - scale/2
		out = append(out, tc{int64(r.Intn(5)), r.Int63n(20000) - 100, truth, miner})
	}
	return out
}

func run(ctx context.Context, wasm []byte, cases []tc) ([]int64, error) {
	rt, err := runtime.NewDeterministicInterruptible(ctx, wasm)
	if err != nil {
		return nil, err
	}
	defer rt.Close(ctx) //nolint:errcheck
	got := make([]int64, len(cases))
	for i, c := range cases {
		if c.truth == 0 {
			continue // the node never passes a zero truth
		}
		s, err := rt.ScoreDeterministic(ctx, c.miner, c.truth, [2]int64{c.code, c.tol})
		if err != nil {
			return nil, fmt.Errorf("case %d %+v: %w", i, c, err)
		}
		got[i] = int64(s)
	}
	return got, nil
}

func main() {
	if len(os.Args) == 5 && os.Args[1] == "pack" {
		if err := packMain(os.Args[2], os.Args[3], os.Args[4]); err != nil {
			fmt.Println("FAIL:", err)
			os.Exit(1)
		}
		return
	}
	if len(os.Args) >= 3 && os.Args[1] == "hash" {
		if err := hashMain(os.Args[2:]); err != nil {
			fmt.Println("FAIL:", err)
			os.Exit(1)
		}
		return
	}
	if len(os.Args) != 2 {
		fmt.Fprintln(os.Stderr, "usage: verify <module.wasm> | verify pack <base.wasm> <out-dir> INTENT,...")
		os.Exit(2)
	}
	wasm, err := os.ReadFile(os.Args[1])
	if err != nil {
		panic(err)
	}
	ctx := context.Background()
	cases := grid()
	first, err := run(ctx, wasm, cases)
	if err != nil {
		fmt.Println("FAIL:", err)
		os.Exit(1)
	}
	second, err := run(ctx, wasm, cases)
	if err != nil {
		fmt.Println("FAIL:", err)
		os.Exit(1)
	}
	mismatch, nondet, maxDev, scored := 0, 0, int64(0), 0
	for i, c := range cases {
		if c.truth == 0 {
			continue
		}
		scored++
		want := comparator.ReferenceScore(c.code, c.tol, c.truth, c.miner)
		if d := first[i] - want; d != 0 {
			if mismatch < 5 {
				fmt.Printf("mismatch %+v: want %d got %d\n", c, want, first[i])
			}
			mismatch++
			if d < 0 {
				d = -d
			}
			if d > maxDev {
				maxDev = d
			}
		}
		if first[i] != second[i] {
			nondet++
		}
	}
	fmt.Printf("cases %d · mismatches %d · max deviation %d bp · nondeterministic %d\n", scored, mismatch, maxDev, nondet)
	if mismatch > 0 || nondet > 0 {
		os.Exit(1)
	}
	fmt.Println("PASS")
}
