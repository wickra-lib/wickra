# `batch_fast()` is the opt-in fast batch: it keeps `NA` placement and shape and
# agrees with `batch()` within a documented tolerance; without a kernel (or
# without a native fast routine at all) it is `batch()` exactly.

prices <- function(n) {
  i <- seq_len(n) - 1
  100 + sin(i * 0.0137) * 5 + cos(i * 0.37)
}

expect_within <- function(exact, fast, tol) {
  expect_identical(dim(exact), dim(fast))
  expect_identical(length(exact), length(fast))
  expect_identical(is.na(exact), is.na(fast))
  ok <- !is.na(exact)
  expect_true(all(abs(exact[ok] - fast[ok]) <= tol * pmax(1, abs(exact[ok]))))
}

n <- 1003

test_that("scalar batch_fast agrees with batch", {
  x <- prices(n)
  expect_within(batch(Sma(20), x), batch_fast(Sma(20), x), 1e-12)
  expect_within(batch(Ema(20), x), batch_fast(Ema(20), x), 1e-12)
  expect_within(batch(Rsi(14), x), batch_fast(Rsi(14), x), 1e-12)
})

test_that("batch_fast without a kernel is the exact batch", {
  x <- prices(n)
  expect_identical(batch(Roc(10), x), batch_fast(Roc(10), x))
})

test_that("batch_fast falls back to batch where there is no fast routine", {
  o <- prices(60)
  h <- o + 1
  l <- o - 1
  c <- o + 0.5
  v <- rep(1000, 60)
  ts <- as.numeric(0:59)
  expect_identical(
    batch(AbandonedBaby(), o, h, l, c, v, ts),
    batch_fast(AbandonedBaby(), o, h, l, c, v, ts)
  )
})

test_that("candle and pair batch_fast agree with batch", {
  cl <- prices(n)
  hi <- cl + 1
  lo <- cl - 1
  vol <- 1000 + (seq_len(n) %% 7)
  ts <- as.numeric(seq_len(n))
  expect_within(batch(Atr(14), cl, hi, lo, cl, vol, ts),
                batch_fast(Atr(14), cl, hi, lo, cl, vol, ts), 1e-12)
  expect_within(batch(ChaikinOscillator(3, 10), cl, hi, lo, cl, vol, ts),
                batch_fast(ChaikinOscillator(3, 10), cl, hi, lo, cl, vol, ts), 1e-9)
  y <- rev(cl) * 0.5 + 3
  expect_within(batch(PearsonCorrelation(20), cl, y),
                batch_fast(PearsonCorrelation(20), cl, y), 1e-9)
})

test_that("multi-output batch_fast agrees with batch", {
  x <- prices(n)
  expect_within(batch(MacdIndicator(12, 26, 9), x), batch_fast(MacdIndicator(12, 26, 9), x), 1e-12)
  expect_within(batch(BollingerBands(20, 2), x), batch_fast(BollingerBands(20, 2), x), 1e-10)
  expect_identical(colnames(batch_fast(MacdIndicator(12, 26, 9), x)),
                   colnames(batch(MacdIndicator(12, 26, 9), x)))
})

test_that("batch_fast validates its columns like batch", {
  expect_error(batch_fast(Sma(3)), "at least one input column")
  expect_error(batch_fast(Sma(3), c("a", "b")), "must be numeric")
  expect_error(batch_fast(PearsonCorrelation(5), 1:10, 1:9), "same length")
})
