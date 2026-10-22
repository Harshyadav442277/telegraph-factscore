// Command verify_wide scores token ids from export_tokens.py with a packaged wide adapter,
// using the node's own runtime (telegraph-wasm-baseline pkg/wasm/runtime.NewWideEmbedded,
// which checks the embedded model's checksum before it scores anything). It then compares each
// score with the float model's score for the same text.
//
// It passes when the package orders every pair the way the float model does and the two sets of
// scores are almost perfectly correlated. Fixed-point rounding may move a score slightly, but it
// must not reorder a pair.
//
//	go run . <INTENT.wasm> <tokens.json>
package main

import (
	"context"
	"encoding/json"
	"fmt"
	"math"
	"os"
	"time"

	"github.com/telegraphprotocol/telegraph-wasm-baseline/pkg/wasm/runtime"
)

type pair struct {
	Kind          string  `json:"kind"`
	ChosenIDs     []int64 `json:"chosen_ids"`
	RejectedIDs   []int64 `json:"rejected_ids"`
	FloatChosen   float64 `json:"float_chosen"`
	FloatRejected float64 `json:"float_rejected"`
}

func pearson(a, b []float64) float64 {
	n := float64(len(a))
	var ma, mb float64
	for i := range a {
		ma += a[i] / n
		mb += b[i] / n
	}
	var sab, saa, sbb float64
	for i := range a {
		sab += (a[i] - ma) * (b[i] - mb)
		saa += (a[i] - ma) * (a[i] - ma)
		sbb += (b[i] - mb) * (b[i] - mb)
	}
	return sab / math.Sqrt(saa*sbb)
}

func main() {
	if len(os.Args) != 3 {
		fmt.Fprintln(os.Stderr, "usage: verify_wide <INTENT.wasm> <tokens.json>")
		os.Exit(2)
	}
	ctx := context.Background()
	pkg, err := os.ReadFile(os.Args[1])
	if err != nil {
		panic(err)
	}
	var pairs []pair
	raw, err := os.ReadFile(os.Args[2])
	if err != nil {
		panic(err)
	}
	if err := json.Unmarshal(raw, &pairs); err != nil {
		panic(err)
	}
	t0 := time.Now()
	rt, err := runtime.NewWideEmbedded(ctx, pkg)
	if err != nil {
		fmt.Println("FAIL: node runtime refused the package:", err)
		os.Exit(1)
	}
	defer rt.Close(ctx) //nolint:errcheck
	fmt.Printf("loaded in %s\n", time.Since(t0).Round(time.Millisecond))

	var wasmScores, floatScores []float64
	wasmWins, agree := 0, 0
	t1 := time.Now()
	for i, p := range pairs {
		c, err := rt.ScoreRaw(ctx, p.ChosenIDs)
		if err != nil {
			fmt.Println("FAIL:", err)
			os.Exit(1)
		}
		r, err := rt.ScoreRaw(ctx, p.RejectedIDs)
		if err != nil {
			fmt.Println("FAIL:", err)
			os.Exit(1)
		}
		wc, wr := float64(c)/(1<<32), float64(r)/(1<<32)
		wasmScores = append(wasmScores, wc, wr)
		floatScores = append(floatScores, p.FloatChosen, p.FloatRejected)
		if wc > wr {
			wasmWins++
		}
		if (wc > wr) == (p.FloatChosen > p.FloatRejected) {
			agree++
		}
		fmt.Printf("%2d %-9s wasm %+.4f / %+.4f   float %+.4f / %+.4f\n", i, p.Kind, wc, wr, p.FloatChosen, p.FloatRejected)
	}
	n := len(pairs)
	r := pearson(wasmScores, floatScores)
	fmt.Printf("pairs %d · wasm orders %d correctly · agrees with float on %d · pearson %.5f · %.1f s per answer\n",
		n, wasmWins, agree, r, time.Since(t1).Seconds()/float64(2*n))
	if agree != n || r < 0.999 {
		os.Exit(1)
	}
	fmt.Println("PASS")
}
