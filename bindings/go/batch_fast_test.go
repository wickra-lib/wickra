package wickra

import (
	"math"
	"testing"
)

// The caller-buffer BatchInto forms and the opt-in BatchFast. BatchInto is bit
// for bit the allocating Batch; BatchFast keeps NaN placement and length and
// agrees within a documented tolerance.

const fastN = 1003

// fastPrices is a wandering path long enough for every kernel's vector loop and tail.
func fastPrices(n int) []float64 {
	p := make([]float64, n)
	for i := range p {
		p[i] = 100 + math.Sin(float64(i)*0.0137)*5 + math.Cos(float64(i)*0.37)
	}
	return p
}

func assertWithin(t *testing.T, exact, fast []float64, tol float64) {
	t.Helper()
	if len(exact) != len(fast) {
		t.Fatalf("length %d vs %d", len(exact), len(fast))
	}
	for i := range exact {
		if math.IsNaN(exact[i]) != math.IsNaN(fast[i]) {
			t.Fatalf("NaN mismatch at %d: %v vs %v", i, exact[i], fast[i])
		}
		if !math.IsNaN(exact[i]) && math.Abs(exact[i]-fast[i]) > tol*math.Max(1, math.Abs(exact[i])) {
			t.Fatalf("at %d: %v vs %v", i, exact[i], fast[i])
		}
	}
}

func assertBits(t *testing.T, a, b []float64) {
	t.Helper()
	if len(a) != len(b) {
		t.Fatalf("length %d vs %d", len(a), len(b))
	}
	for i := range a {
		if math.Float64bits(a[i]) != math.Float64bits(b[i]) {
			t.Fatalf("bits differ at %d: %v vs %v", i, a[i], b[i])
		}
	}
}

func mustPanic(t *testing.T, f func()) {
	t.Helper()
	defer func() {
		if recover() == nil {
			t.Fatal("expected a panic")
		}
	}()
	f()
}

func TestBatchIntoIsTheAllocatingBatchBitForBit(t *testing.T) {
	input := fastPrices(fastN)
	a, _ := NewEma(20)
	defer a.Close()
	b, _ := NewEma(20)
	defer b.Close()
	want := a.Batch(input)
	got := make([]float64, fastN)
	b.BatchInto(got, input)
	assertBits(t, want, got)
	if a.Update(101) != b.Update(101) {
		t.Fatal("state diverged after BatchInto")
	}
}

func TestScalarBatchFastAgreesWithBatch(t *testing.T) {
	input := fastPrices(fastN)
	exact, _ := NewSma(20)
	defer exact.Close()
	fast, _ := NewSma(20)
	defer fast.Close()
	assertWithin(t, exact.Batch(input), fast.BatchFast(input), 1e-12)
	if math.Abs(exact.Update(101)-fast.Update(101)) > 1e-12 {
		t.Fatal("streaming after BatchFast drifted")
	}

	rsiExact, _ := NewRsi(14)
	defer rsiExact.Close()
	rsiFast, _ := NewRsi(14)
	defer rsiFast.Close()
	reused := make([]float64, fastN)
	rsiFast.BatchFastInto(reused, input)
	assertWithin(t, rsiExact.Batch(input), reused, 1e-12)
}

func TestBatchFastWithoutAKernelIsTheExactBatch(t *testing.T) {
	input := fastPrices(fastN)
	exact, _ := NewRoc(10)
	defer exact.Close()
	fast, _ := NewRoc(10)
	defer fast.Close()
	assertBits(t, exact.Batch(input), fast.BatchFast(input))
}

func TestBatchIntoRejectsAMismatchedDestination(t *testing.T) {
	input := fastPrices(16)
	sma, _ := NewSma(3)
	defer sma.Close()
	mustPanic(t, func() { sma.BatchInto(make([]float64, 15), input) })
	mustPanic(t, func() { sma.BatchFastInto(make([]float64, 17), input) })
	pearson, _ := NewPearsonCorrelation(5)
	defer pearson.Close()
	mustPanic(t, func() { pearson.BatchFast(input, make([]float64, 15)) })
}

func TestEmptyBatchesAreEmpty(t *testing.T) {
	ema, _ := NewEma(5)
	defer ema.Close()
	if len(ema.Batch(nil)) != 0 || len(ema.BatchFast(nil)) != 0 {
		t.Fatal("empty input gave output")
	}
	ema.BatchFastInto(nil, nil)
	macd, _ := NewMacdIndicator(12, 26, 9)
	defer macd.Close()
	if len(macd.BatchFast(nil)) != 0 {
		t.Fatal("empty MACD input gave output")
	}
}

func TestCandleBatchFastAgreesWithBatch(t *testing.T) {
	closes := fastPrices(fastN)
	opens := append([]float64(nil), closes...)
	highs := make([]float64, fastN)
	lows := make([]float64, fastN)
	volumes := make([]float64, fastN)
	stamps := make([]int64, fastN)
	for i := range closes {
		highs[i] = closes[i] + 1
		lows[i] = closes[i] - 1
		volumes[i] = 1000 + float64(i%7)
		stamps[i] = int64(i)
	}
	atrExact, _ := NewAtr(14)
	defer atrExact.Close()
	atrFast, _ := NewAtr(14)
	defer atrFast.Close()
	assertWithin(t,
		atrExact.Batch(opens, highs, lows, closes, volumes, stamps),
		atrFast.BatchFast(opens, highs, lows, closes, volumes, stamps), 1e-12)

	adoscExact, _ := NewChaikinOscillator(3, 10)
	defer adoscExact.Close()
	adoscFast, _ := NewChaikinOscillator(3, 10)
	defer adoscFast.Close()
	assertWithin(t,
		adoscExact.Batch(opens, highs, lows, closes, volumes, stamps),
		adoscFast.BatchFast(opens, highs, lows, closes, volumes, stamps), 1e-9)
}

func TestPairBatchFastAgreesWithBatch(t *testing.T) {
	x := fastPrices(fastN)
	y := make([]float64, fastN)
	for i := range y {
		y[i] = x[fastN-1-i]*0.5 + 3
	}
	exact, _ := NewPearsonCorrelation(20)
	defer exact.Close()
	fast, _ := NewPearsonCorrelation(20)
	defer fast.Close()
	assertWithin(t, exact.Batch(x, y), fast.BatchFast(x, y), 1e-9)
}

func TestMultiOutputBatchFastAgreesWithBatch(t *testing.T) {
	input := fastPrices(fastN)
	macdExact, _ := NewMacdIndicator(12, 26, 9)
	defer macdExact.Close()
	macdFast, _ := NewMacdIndicator(12, 26, 9)
	defer macdFast.Close()
	want, got := macdExact.Batch(input), macdFast.BatchFast(input)
	for i := range want {
		assertWithin(t,
			[]float64{want[i].Macd, want[i].Signal, want[i].Histogram},
			[]float64{got[i].Macd, got[i].Signal, got[i].Histogram}, 1e-12)
	}

	bbExact, _ := NewBollingerBands(20, 2)
	defer bbExact.Close()
	bbFast, _ := NewBollingerBands(20, 2)
	defer bbFast.Close()
	bw, bg := bbExact.Batch(input), bbFast.BatchFast(input)
	for i := range bw {
		assertWithin(t,
			[]float64{bw[i].Upper, bw[i].Middle, bw[i].Lower, bw[i].Stddev},
			[]float64{bg[i].Upper, bg[i].Middle, bg[i].Lower, bg[i].Stddev}, 1e-10)
	}
}
