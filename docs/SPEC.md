# fika protocol specification

Version 0.2 (draft, "v2 waveform"). Normative. The rationale is in
[DESIGN.md](DESIGN.md). The reference implementation lives in `crates/` of
this repository; where the two disagree, the implementation is the bug until
this document is changed. Version 0.1 (16-tone, 500 Hz lanes) is obsolete and
kept only in the git history.

The key words MUST, MUST NOT, SHOULD and MAY are to be read as in RFC 2119.

## 1. Scope and terminology

fika is a store-and-send text messaging mode for HF amateur radio. A
transmitting station sends a complete message as one burst. Receiving
stations decode bursts asynchronously, without any shared clock, and decode
**several bursts that overlap in time in the same band**.

| Term | Meaning |
|---|---|
| Tone | One of 64 audio frequencies spanning 300–2700 Hz, numbered 0..63. |
| Symbol | One tone transmitted for one symbol period. Carries one GF(64) code symbol, 6 bits. |
| Profile | The symbol period: **F** (fast, 26.67 ms) or **S** (slow, 160 ms). |
| Preamble | The fixed 24-symbol sequence that starts every burst. |
| Block | One LDPC codeword on air (128 symbols) with its 4 pilot symbols. |
| Burst | One transmission: preamble followed by 1..8 blocks. |
| Frame | The decoded content of a burst: a message, an ACK or a beacon. |
| Long frame | A message burst. Short frame: an ACK or beacon, always one block. |

Bit strings are written most significant bit first. Bit fields are packed in
the order listed, MSB first.

## 2. Properties

| Property | Profile F | Profile S |
|---|---|---|
| Occupied bandwidth | 2400 Hz (300–2700 Hz) | same |
| Tones | 64 at 37.5 Hz spacing | same |
| Symbol period / rate | 26.67 ms, 37.5 Bd | 160 ms, 6.25 Bd |
| Raw bit rate | 225 bit/s | 37.5 bit/s |
| Coded bit rate (rate-1/2 LDPC) | 112.5 bit/s | 18.75 bit/s |
| Block airtime (132 symbols) | 3.52 s | 21.1 s |
| Sensitivity, AWGN, 50 % decode, **measured** (`fika sim`) | **−10.2 dB** | **−19.3 dB** |
| Sensitivity, theory (symbol-level capacity + code) | −10.9 dB | −18.7 dB |
| Simultaneous senders in one band, equal power, measured | 2, 3 and 4 at 100 % (−6 dB each); 3 at 100 % at −8 dB | not yet measured |
| Senders 10 dB apart, weak at −8 dB, measured | weak decodes 95 % | not yet measured |
| Maximum message | 8 blocks, 2725 payload bits, about 240 characters of ordinary text | same |
| Airtime, 40-character message (1 block) | 4.2 s | 25 s |
| Airtime, 240-character message (2 blocks) | 7.7 s | 46 s |
| Airtime, ACK or beacon | 4.2 s | 25 s |
| Frequency tolerance | ±75 Hz search, ±20 Hz recommended | same |
| Clock requirement | none | none |
| FEC | GF(64) LDPC, n = 128, k = 64, dv = 2, dc = 4 | same |
| Text coding | order-1 adaptive range coder | same |

SNR is referenced to a 2500 Hz bandwidth, as FT8 reports it. Measured figures
come from the simulator in `crates/fika-cli` with the default thresholds.

## 3. Tone plan

### 3.1 Frequencies

    f(k) = 318.75 + 37.5 · k   Hz,   k = 0..63

The comb is centred on 1500 Hz; tone 0 minus half a spacing is 300 Hz and
tone 63 plus half a spacing is 2700 Hz. Transmitters and receivers MUST use
the rig's widest data filter (IC-705 "wide", FT-891 3000 Hz). A 2.4 kHz
filter attenuates the outer tones by 1–3 dB, which the per-bin normalisation
of §8.2 tolerates.

All tones lie on the 37.5 Hz grid, which at the 12 kHz receiver rate is
exactly 320 samples per fast symbol.

### 3.2 Pulse shaping

The instantaneous frequency is the tone sequence convolved with the
Gaussian pulse

    p(t) = ½ [ erf(κ (t + ½)) − erf(κ (t − ½)) ],   κ = π · sqrt(2 / ln 2) · BT

with `BT = 2.0` and `t` in units of **one fast symbol (26.67 ms) for both
profiles**. In profile S the tone is held flat for most of the 160 ms symbol;
only the 26.67 ms around each transition is shaped. The phase MUST be
continuous and the amplitude constant for the whole burst apart from
raised-cosine ramps of one unit at each end.

### 3.3 Frequency error

The receiver searches ±2 tones (±75 Hz) around nominal. A transmitter SHOULD
be within ±20 Hz.

## 4. Symbols and profiles

| Profile | Symbol period | Samples at 12 kHz | Samples at 48 kHz |
|---|---|---|---|
| F | 26.67 ms (1 unit) | 320 | 1280 |
| S | 160 ms (6 units) | 1920 | 7680 |

Every data symbol carries one GF(64) code symbol `d` (6 bits, MSB first in
the byte stream of §9). A burst uses one profile throughout. A receiver MUST
run the detectors for both profiles.

## 5. Hop pattern and preamble sequences

### 5.1 Sequences

Three Welch Costas arrays of order 16 from the prime 17 mark the preamble
parts, and one order-64 array drives the data hop:

    W_g[i] = (g^i mod 17) − 1,   i = 0..15
    LONG  = W_3 = [0, 2, 8, 9, 12, 4, 14, 10, 15, 13, 7, 6, 3, 11, 1, 5]
    SHORT = W_6 = [0, 5, 1, 11, 3, 6, 7, 13, 15, 10, 14, 4, 12, 9, 8, 2]
    PILOT = W_7 = [0, 6, 14, 2, 3, 10, 8, 11, 15, 9, 1, 13, 12, 5, 7, 4]

    H64: y_i = 4 · 2^i mod 67 for i = 0..65, keep y ≥ 3, x = y − 3 (64 values)
    H64 = [1, 5, 13, 29, 61, 58, 52, 40, 16, 35, 6, 15, 33, 2, 7, 17, 37, 10,
           23, 49, 34, 4, 11, 25, 53, 42, 20, 43, 22, 47, 30, 63, 62, 60, 56,
           48, 32, 0, 3, 9, 21, 45, 26, 55, 46, 28, 59, 54, 44, 24, 51, 38,
           12, 27, 57, 50, 36, 8, 19, 41, 18, 39, 14, 31]

All four are Costas arrays: their two-dimensional aperiodic autocorrelation
is at most 1 for every non-zero shift. The order-16 arrays are laid on the
64-tone band scaled by 4 with a residue that keeps the three roles on
disjoint tone sets:

    long SYNC tone   = 4 · LONG[n]
    short SYNC tone  = 4 · SHORT[n] + 2
    PHASE/pilot tone = 4 · PILOT[(n + φ) mod 16] + 1

### 5.2 Pattern phase

Every burst has a pattern phase `φ` in 0..15, drawn uniformly at random per
burst (ACKs use `φ = msg_id mod 16`).

### 5.3 Data and pilot symbol mapping

Number the symbols after the preamble `m = 0, 1, 2, ...`, pilots included.

    data tone  t_m = ( d_m + H64[(m + 4φ) mod 64] ) mod 64
    pilot tone t_m = 4 · PILOT[(m + φ) mod 16] + 1

### 5.4 What the pattern guarantees

Overlapping preambles with different `φ` or timing coincide on at most one
SYNC symbol, so each is detected and timed separately. Data tones of
different bursts are effectively random with respect to each other; what
makes overlapping bursts decodable is the symbol-level decoding of §8, not
the hop pattern. The hop additionally turns a steady interferer into
scattered erasures and spreads every burst over the whole band for frequency
diversity.

## 6. Preamble and synchronisation

### 6.1 Structure

| Part | Symbols | Tone of symbol n |
|---|---|---|
| SYNC | 16 | `4·LONG[n]` (long) or `4·SHORT[n] + 2` (short) |
| PHASE | 8 | `4·PILOT[(n + φ) mod 16] + 1` |

Airtime 0.64 s (F), 3.84 s (S).

### 6.2 Coarse detection

For each profile the receiver maintains a normalised energy matrix
`Ẽ[frame, bin]` (§10), clipped at 20 for this step. For each sequence in
{LONG, SHORT}, each frequency offset `ν` within ±2 tones (quarter-tone steps
in F, twelfth-tone steps in S) and each start frame `τ` (quarter-symbol
steps):

    Z(τ, ν, seq) = Σ_{n=0..15} Ẽ[τ + n·T_s, ν + tone(seq, n)]

Candidates have `Z ≥ 40` (noise mean 16, σ 4).

### 6.3 Fine pass

On unclipped energies, every candidate MUST pass:

1. Re-find the peak `Z_u` within ±1 symbol and the whole frequency range.
2. **Peak test:** `Z_u ≥ 2.0 ×` the larger of `Z_u` one symbol earlier and later.
3. **Support test:** at least 11 of the 16 SYNC cells at or above a quarter of their mean.
4. **Median test:** the median of the 16 cells at least 2.5 (noise median ≈ 0.7).
5. Parabolic interpolation of `τ` and `ν`; then suppress other candidates of
   the same kind within ±1 symbol and ±1 tone (only).
6. **PHASE:** score all 16 `φ` on unclipped energies clipped at twice the
   mean SYNC peak; report **every** `φ` whose score is at least
   `8·(1 + 0.5·γ̂)` and at least 0.6 × the best, so two bursts keyed within a
   symbol yield two detections.
7. **Timing:** refine `τ` in the sample domain over ±1/8 symbol in steps of
   1/64 symbol by the summed energy of the 16 SYNC tones.

`γ̂ = Z_u / 16 − 1`; the reported Es/N0 is `1.38·γ̂` (empirical calibration).

### 6.4 Profile and kind

Profile is told apart by symbol rate: each detector scores the other
profile's preamble poorly. Kind is told apart by which sequence matched.

### 6.5 Tracking

Each block begins with 4 pilot symbols. After each successfully decoded block
the receiver SHOULD re-estimate `τ` and `ν` from the re-encoded block,
bounded to ±T_s/4 and ±1/4 tone per block.

## 7. Blocks and frames

### 7.1 Code

One code for all frames: the GF(64) LDPC of §8.1 with n = 128 coded symbols
and k = 64 information symbols = **384 bits = 48 bytes** per block. Block on
air = 4 pilots + 128 data symbols = 132 symbols: 3.52 s (F), 21.1 s (S).

### 7.2 Burst layout

    preamble (24) | block 0 (132) | block 1 (132) | ... | block N−1

Blocks follow each other with no gap. Block `k` starts `24 + 132·k` symbols
after the burst start. Long frames have 1..8 blocks; short frames one.

### 7.3 Long frame, block 0 (message header)

| Field | Bits | Meaning |
|---|---|---|
| ver | 2 | 0 |
| type | 3 | 0 = message, 1 = ACK, 2 = beacon, 3–7 reserved |
| sender | 28 | Sender callsign, §9.1 |
| dest_type | 2 | 0 = all, 1 = group, 2 = callsign, 3 reserved |
| dest | 28 | Group ID, packed callsign, or 0 |
| msg_id | 16 | Random per message |
| total | 3 | Number of blocks minus 1 |
| hop | 2 | MUST be 0 in version 0 |
| flags | 2 | bit 1: ack_req; bit 0: raw_text |
| payload | **282** | Start of the text payload, §9.4 |
| crc | 16 | CRC-16 over the preceding 368 bits |

### 7.4 Long frame, blocks 1..7

| Field | Bits |
|---|---|
| seq | 3 |
| msg_id | 16 |
| payload | **349** |
| crc | 16 |

Payload capacity by block count: 282, 631, 980, 1329, 1678, 2027, 2376,
2725 bits.

### 7.5 Short frames

ACK: ver 2 | type 3 | sender 28 | dest 28 | msg_id 16 | snr 6 | reserved 285
| crc 16. Beacon: ver 2 | type 3 | sender 28 | grid 15 | group_tags 24 |
status 4 | reserved 292 | crc 16. Reserved bits MUST be 0.

### 7.6 CRC

CRC-16/CCITT-FALSE (poly 0x1021, init 0xFFFF, no reflection) over the
preceding fields MSB first. A block whose CRC fails MUST be discarded.

### 7.7 Worked airtime

| Message | Bits | Blocks | F | S |
|---|---|---|---|---|
| 40 chars | ≈ 106 | 1 | 0.64 + 3.52 = 4.2 s | 3.84 + 21.1 = 25 s |
| 240 chars | ≈ 606 | 2 | 0.64 + 7.04 = 7.7 s | 46 s |
| ACK / beacon | 384 | 1 | 4.2 s | 25 s |

## 8. Forward error correction and demodulation

### 8.1 Code

A non-binary LDPC code over GF(2^6) with field polynomial x^6 + x + 1,
n = 128, k = 64, every variable node of degree 2 and every check node of
degree 4. Its Tanner graph is the line graph of a 4-regular *check graph* on
64 vertices built by progressive edge growth to girth 6; edge coefficients
are drawn so that no check-graph cycle of length ≤ 8 has a coefficient-ratio
product of 1. Construction is deterministic from the seed `0x6f696b61` with
the splitmix64 generator in `crates/fika-nb/src/rng.rs`; the information set
and systematic generator follow from Gaussian elimination over GF(64). The
code is thus a pure function of this specification. (A frozen table will
replace the procedure in a later revision.)

Encoding: the 48 payload bytes are split MSB first into 64 six-bit symbols;
the 64 parity symbols are appended at the code's parity positions.

An opt-in rate-1/3 "crowd" code (n = 192, k = 64, dc = 3, seed
`0x63726f77`) is defined for later use; it is not transmitted by this
version.

### 8.2 Symbol likelihoods

For each data symbol the receiver computes the energy `E_t` in each of the
64 tone positions at the estimated `(τ, ν)`. Each tone bin is normalised by
its own noise level `n_t`, the 75th percentile of that bin's energy over the
block divided by ln 4: the hop places the wanted signal on any one bin only
one symbol in 64, so this measures the bin's noise plus whatever interferer
sits on it. With `e_t = E_t / n_t`, `γ̂` the block's mean peak minus one,
capped at 15 dB, and `q̂` the fraction of cells above four times the noise
beyond one per symbol (floor 1/64):

    ℓ_t = ln I0(2·sqrt(γ̂·e_t)) − γ̂
    L_t = ℓ_t − ln(1 − q̂ + q̂·e^{ℓ_t})
    P(d) ∝ exp(L_{tone(d)})

`L_t` saturates at ln(1/q̂): two equal peaks split the posterior, and a much
stronger peak gets no more credit than the wanted one. The hop offset is
removed by permuting the vector, and the 128 vectors go to the decoder.

### 8.3 Decoding

Sum-product belief propagation over GF(64): messages are probability
vectors; check nodes permute by the edge coefficient, Walsh–Hadamard
transform, multiply, inverse transform (in f64), and un-permute. At most 50
iterations; a codeword is accepted only if all 64 checks are satisfied, and
the frame only if its CRC verifies.

## 9. Content coding

### 9.1 Callsigns

Standard callsigns are packed into 28 bits using the FT8 six-character
index (Franke, Somerville and Taylor, QEX July/August 2020) without FT8's
token and hash offsets, giving values below 262 177 560 = 37·36·10·27³.
Other callsigns are sent as 262 177 560 plus the low 22 bits of the FNV-1a
hash of their uppercase ASCII text.

### 9.2 Groups

The on-air group ID is the low 28 bits of FNV-1a over the NFC, lower-cased,
UTF-8 group name; beacons carry its low 12 bits.

### 9.3 Message ID

16 random bits per message. `(sender, msg_id)` identifies a message for
de-duplication and acknowledgement; receivers keep it for at least an hour.

### 9.4 Text payload

A 32-bit range coder with an order-1 adaptive model over a 119-symbol
alphabet (ASCII, newline, Nordic letters and common punctuation, plus ESC,
REP, EOT), initialised from the static prior table in
`crates/fika-proto/src/prior.rs`, increment 24, halving above 4096. Code
points outside the alphabet are ESC plus 21 raw bits; REP repeats the last
escaped code point. The payload ends with EOT and two flush bits. If the
coded payload exceeds 8 bits per UTF-8 byte the sender sets `raw_text` and
sends the UTF-8 bytes followed by a zero byte. A payload MUST NOT exceed
2725 bits.

## 10. Receiver reference pipeline

1. Audio at 12 kHz (decimate from 48 kHz by 4, or capture at 12 kHz).
2. Energy matrix F: window 320, zero-padded to 1280 (9.375 Hz bins, 4 per
   tone), hop 80. S: window 1920 → 3840 (3.125 Hz, 12 per tone), hop 480.
3. Per-bin baseline: 30th percentile over 8 s (F) / 40 s (S) blocks scaled
   by 1/−ln 0.7, floored at a thousandth of the bin's maximum and of the
   global mean.
4. Detection as §6; demodulation as §8 with tone energies from 64 complex
   correlations over the symbol (320 samples).
5. Overlapping bursts are decoded independently; successive cancellation
   (decode the strongest, blank or subtract it, decode the rest) is
   OPTIONAL and specified in a later revision.

Estimated load on one Raspberry Pi 4 core: about 20 MFLOP/s of FFT, under
0.1 M adds/s of correlation, and 5–20 ms per GF(64) block decode.

## 11. Transmitter reference pipeline

As version 0.1: synthesise at the sound-card rate, phase-continuous,
constant amplitude −6 dBFS, 26.67 ms raised-cosine ramps, PTT via rigctld
with a configurable delay. The audio chain MUST NOT clip.

## 12. Channel access

Listen-before-talk is OPTIONAL and off by default: HF is never quiet, and
overlapping bursts are decodable. A station that enables it treats the band
as busy while a detected burst is in progress, while the hottest tone bin is
10 dB above the passband median, or during a reserved ACK window, and backs
off a random 0..7 slots of 0.5 s.

## 13. Acknowledgement, presence, relay

Unchanged from version 0.1 in substance: a direct message with `ack_req`
set is answered by an ACK short frame on the same profile from 1 s after the
burst; group messages are not acknowledged; every decoded frame updates the
heard list; beacons are optional; relay is reserved (`hop` = 0).

## 14. Regulatory notes

The emission is 64-tone MFSK with a necessary bandwidth of 2400 Hz: a wide
digital mode, for the IARU Region 1 wide-digimode segments and, in the US,
within the 2.8 kHz data bandwidth limit at 37.5 Bd. The per-symbol tone
permutation does not expand the bandwidth. This document is the public
specification of the code.
