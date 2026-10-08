# fika protocol specification

Version 0.1 (draft). Normative. The rationale is in [DESIGN.md](DESIGN.md).

The key words MUST, MUST NOT, SHOULD and MAY are to be read as in RFC 2119.

## 1. Scope and terminology

fika is a store-and-send text messaging mode for HF amateur radio. A
transmitting station sends a complete message as one burst. Receiving
stations decode bursts asynchronously, without any shared clock.

| Term | Meaning |
|---|---|
| Bin | A 31.25 Hz slice of the audio passband. All tone frequencies lie on the bin grid. |
| Tone | One of 16 audio frequencies in a lane, numbered 0..15 from the lowest. |
| Lane | A 500 Hz wide sub-channel holding 16 tones. Up to 4 lanes fit in a 300–2700 Hz SSB passband. |
| Symbol | One tone transmitted for one symbol period. Carries 4 coded bits. |
| Profile | The symbol period: **F** (fast, 32 ms) or **S** (slow, 160 ms). |
| Preamble | The fixed 24-symbol sequence that starts every burst. |
| Block | One LDPC codeword on air, with its 4 pilot symbols. |
| Burst | One transmission: preamble followed by 1..8 blocks. |
| Frame | The decoded content of a burst: a message, an ACK or a beacon. |
| Long frame | A message burst, built from (512,256) blocks. |
| Short frame | An ACK or beacon burst, one (256,128) block. |

Bit strings are written most significant bit first. Bit fields are packed in
the order listed, MSB first, with no padding unless stated.

## 2. Properties

| Property | Profile F | Profile S |
|---|---|---|
| Occupied bandwidth (99 % power) | 500 Hz | 500 Hz |
| Tones | 16 at 31.25 Hz spacing | same |
| Symbol period | 32 ms | 160 ms |
| Symbol rate | 31.25 Bd | 6.25 Bd |
| Raw bit rate | 125 bit/s | 25 bit/s |
| Coded bit rate (after rate 1/2 LDPC) | 62.5 bit/s | 12.5 bit/s |
| Net text rate, 4-block message | ≈ 46 bit/s ≈ 13 char/s | ≈ 9 bit/s ≈ 2.7 char/s |
| Sensitivity, AWGN, 50 % decode, 2500 Hz reference | −12.5 dB | −19.5 dB |
| Sensitivity, Watterson CCIR moderate (estimate) | ≈ −10 dB | ≈ −17 dB |
| Maximum message | 8 blocks, 1701 payload bits, about 240 characters of ordinary text | same |
| Airtime, 40-character message | 5.0 s | 25 s |
| Airtime, 240-character message | 17.7 s | 88 s |
| Airtime, ACK or beacon | 2.9 s | 14.7 s |
| Lanes per 300–2700 Hz passband | 4 | 4 |
| Concurrent stations per lane | 1 at equal power. A stronger station decodes through an overlap of ≥ 6 dB; the weaker one after successive cancellation. Partial overlap of ≤ 20 % of a block is tolerated at equal power. | same |
| Frequency tolerance | ±62.5 Hz search range, ±20 Hz recommended | same |
| Clock requirement | none | none |
| FEC | CCSDS TC (512,256) long, (256,128) short, soft BP | same |
| Text coding | order-1 adaptive range coder, ≈ 2.5 bit/char | same |

Sensitivity figures are derived in Section 8 and MUST be confirmed by
simulation before being quoted as measured properties of the mode.

## 3. Lane and tone plan

### 3.1 Tone frequencies

Tone `k` (0..15) of lane `L` (0..3) is at

    f(L, k) = 437.5 + 562.5 · L + 31.25 · k   Hz

All tones lie on the 31.25 Hz bin grid (437.5 Hz = bin 14). Lane pitch is
562.5 Hz = 18 bins: 16 tone bins and 2 guard bins (62.5 Hz).

| Lane | Tone 0 | Tone 15 | Centre | Occupied span (±half bin) |
|---|---|---|---|---|
| 0 | 437.50 | 906.25 | 671.875 | 421.9 – 921.9 |
| 1 | 1000.00 | 1468.75 | 1234.375 | 984.4 – 1484.4 |
| 2 | 1562.50 | 2031.25 | 1796.875 | 1546.9 – 2046.9 |
| 3 | 2125.00 | 2593.75 | 2359.375 | 2109.4 – 2609.4 |

Margins to a 300–2700 Hz passband: 122 Hz below lane 0, 91 Hz above lane 3.

A transmitter MUST use one lane for an entire burst. A receiver MUST monitor
all four lanes.

### 3.2 Pulse shaping

The instantaneous frequency is the tone sequence convolved with the FT8-style
Gaussian pulse

    p(t) = ½ [ erf(κ (t + ½)) − erf(κ (t − ½)) ],   κ = π · sqrt(2 / ln 2) · BT

with `BT = 2.0` and `t` in units of **32 ms for both profiles**. The pulse
support is 3 units (96 ms). In profile S the tone is therefore held flat for
most of the 160 ms symbol and only the 32 ms around each transition is shaped.
`BT` MUST NOT be scaled with the symbol period.

The phase MUST be continuous across tone changes and the amplitude MUST be
constant for the whole burst, apart from the ramps in Section 11.

Resulting spectrum: 99 % of power within 500 Hz including shaping skirts,
about −40 dBc one bin outside the outer tones, below −60 dBc beyond about
90 Hz outside. The −60 dB bandwidth of roughly 570 Hz is absorbed by the
62.5 Hz guard bins.

### 3.3 Frequency error

The receiver searches ±2 bins (±62.5 Hz) around each lane's nominal tone 0. A
transmitter SHOULD be within ±20 Hz of nominal. Up to ±31 Hz of error moves
the outer tone into the guard band, not into the neighbouring lane.

## 4. Symbols and profiles

| Profile | Symbol period T_s | Samples at 12 kHz | Samples at 48 kHz |
|---|---|---|---|
| F | 32 ms | 384 | 1536 |
| S | 160 ms | 1920 | 7680 |

Tone spacing is 31.25 Hz in both profiles: modulation index 1 in F and 5 in S.
Every symbol carries 4 coded bits `b0 b1 b2 b3` (MSB first) as the data value
`d = 8·b0 + 4·b1 + 2·b2 + b3`.

A burst uses one profile throughout. A receiver MUST run the detectors for
both profiles on every lane.

## 5. Hop pattern

### 5.1 Base sequence

The base sequence `C` is the Welch Costas array of order 16 from the prime 17
and primitive root 3:

    C[i] = (3^i mod 17) − 1,   i = 0..15

    C = [0, 2, 8, 9, 12, 4, 14, 10, 15, 13, 7, 6, 3, 11, 1, 5]

`C` is a permutation of 0..15 with the Costas property: its two-dimensional
aperiodic autocorrelation is at most 1 for every non-zero shift in time and
frequency. Because the Welch construction is singly periodic, every cyclic
time shift `C_φ[n] = C[(n + φ) mod 16]` is also a Costas array, and two
distinct shifts agree in at most one position.

The flipped sequence `C' = 15 − C` is also Costas and is used to mark short
frames:

    C' = [15, 13, 7, 6, 3, 11, 1, 5, 0, 2, 8, 9, 12, 4, 14, 10]

### 5.2 Pattern phase

Every burst has a pattern phase `φ` in 0..15.

- For message and beacon bursts the transmitter MUST draw `φ` uniformly at
  random per burst.
- For ACK bursts `φ = msg_id mod 16` of the message being acknowledged.

### 5.3 Data symbol mapping

Number the symbols after the preamble `m = 0, 1, 2, ...`, counting pilot
symbols. The transmitted tone for symbol `m` with data value `d_m` is

    t_m = ( d_m + C[(m + φ) mod 16] ) mod 16

The receiver inverts this after estimating `φ` from the preamble.

### 5.4 What the pattern guarantees

- Two overlapping preambles with different `φ` or different timing coincide
  on at most one symbol of sixteen, so both are detected and timed
  independently.
- A narrowband interferer hits a different data value every symbol, so it
  appears to the decoder as random erasures rather than a stuck bit position.
- Data symbols of two overlapping bursts are effectively random with respect
  to each other. An interferer of comparable power creates a second energy
  peak on every overlapped symbol, which destroys about 2 of the 4 coded bits
  of that symbol. The pattern does not orthogonalise data; it randomises the
  damage and makes successive cancellation possible. See Section 2 for the
  resulting limits.

## 6. Preamble and synchronisation

### 6.1 Structure

Every burst begins with 24 symbols in the burst's profile:

| Part | Symbols | Tone of symbol n |
|---|---|---|
| SYNC | 16 (n = 0..15) | `C[n]` for a long frame, `C'[n]` for a short frame |
| PHASE | 8 (n = 0..7) | `C[(n + φ) mod 16]` |

SYNC is independent of `φ`, so timing and frequency are found first; PHASE
then identifies `φ`. Any two values of `φ` differ in at least 7 of the 8 PHASE
positions.

Preamble airtime: 0.768 s in F, 3.84 s in S.

### 6.2 Detection

The receiver maintains for each profile a normalised energy matrix `Ẽ[frame,
bin]` (Section 10). For each lane, each profile, each sequence in {C, C'},
each candidate frequency offset `ν` within ±2 bins and each candidate start
time `τ`, it computes

    Z(τ, ν, seq) = Σ_{n=0..15} Ẽ[τ + n·T_s, ν + seq[n]]

Time steps are T_s/4 and frequency steps are one quarter bin (F) or one tenth
bin (S).

Under noise alone `Ẽ` is approximately exponential with unit mean, so `Z` is
Gamma(16, 1): mean 16, standard deviation 4. A detection threshold of
`Z ≥ 40` gives a false-alarm probability near 5·10⁻⁶ per test, about one
false preamble per hundred seconds per lane, each costing one failed LDPC
decode. At the decode threshold (Es/N0 ≈ 6.5 dB) the mean of `Z` is about 88
with standard deviation 13, so the miss probability is negligible. Preamble
detection is not the sensitivity limit.

### 6.3 Refinement

After a detection the receiver SHOULD refine `τ` and `ν` by parabolic
interpolation of `Z` around the peak, to about T_s/16 in time and 1/16 bin
(≈ 2 Hz) in frequency for F, 0.6 Hz for S. It then evaluates the PHASE sum for
all 16 values of `φ` at the refined `(τ, ν)` and accepts the maximum if it is
at least twice the second best; otherwise the detection is dropped.

### 6.4 Profile and frame type

Profile is distinguished by symbol rate alone: the F detector sees an S
preamble as runs of five identical tones and scores at most 2–3 Costas hits;
the S detector sees an F preamble spread over five tones per frame. Frame
type (long or short) is given by which sequence, `C` or `C'`, matched.

### 6.5 Tracking

Each block begins with 4 pilot symbols with `d = 0`, so their tones are the
hop offsets and are known once `φ` is known. After each successfully decoded
block the receiver SHOULD re-encode the block, giving all 132 tones, and
re-estimate `τ` and `ν` by maximising energy over them, bounded to ±T_s/4 and
±1/4 bin per block. This tracking is REQUIRED for bursts longer than one
block: a sound card clock error of 100 ppm walks timing by about 9 ms over an
88 s burst, and profile S needs frequency within about 1.5 Hz.

## 7. Blocks and frames

### 7.1 Codes

| Frame | Code | Info bits | Coded bits | Data symbols | Pilots | Symbols per block |
|---|---|---|---|---|---|---|
| Long | CCSDS TC (512,256) | 256 | 512 | 128 | 4 | 132 |
| Short | CCSDS TC (256,128) | 128 | 256 | 64 | 4 | 68 |

These are the telecommand codes as implemented in the `labrador-ldpc` crate.
Coded bits are mapped to symbols 4 at a time, MSB first, after the interleaver
in Section 8.3.

Block airtime: long 4.224 s (F) / 21.12 s (S); short 2.176 s (F) / 10.88 s (S).

### 7.2 Burst layout

    preamble (24) | block 0 (132 or 68) | block 1 (132) | ... | block N−1

Blocks follow each other with no gap. Block `k` of a long frame starts
`24 + 132·k` symbols after the burst start. A long frame has 1..8 blocks; a
short frame has exactly one.

### 7.3 Long frame, block 0 (message header)

| Field | Bits | Meaning |
|---|---|---|
| ver | 2 | Protocol version, 0 for this document |
| type | 3 | 0 = message. 1 and 2 are used by short frames. 3–7 reserved |
| sender | 28 | Sender callsign, Section 9.1 |
| dest_type | 2 | 0 = all, 1 = group, 2 = callsign, 3 reserved |
| dest | 28 | Group ID, packed callsign, or 0 for all |
| msg_id | 16 | Random per message, drawn by the sender |
| total | 3 | Number of blocks in the burst minus 1 (0..7) |
| hop | 2 | Hop count, MUST be 0 in version 0 |
| flags | 2 | bit 1: ack_req; bit 0: raw_text |
| payload | 154 | Start of the text payload, Section 9.4 |
| crc | 16 | CRC-16 over the preceding 240 bits |

Total 256 bits.

### 7.4 Long frame, blocks 1..7 (continuation)

| Field | Bits | Meaning |
|---|---|---|
| seq | 3 | Block index 1..7 |
| msg_id | 16 | Same as block 0 |
| payload | 221 | Continuation of the text payload |
| crc | 16 | CRC-16 over the preceding 240 bits |

Total 256 bits. Continuation blocks are bound to block 0 by contiguous timing
and matching `msg_id`. If block 0 fails, continuation blocks MAY be shown as a
partial message from an unknown sender.

Payload capacity by block count: 154, 375, 596, 817, 1038, 1259, 1480, 1701
bits.

### 7.5 Short frame: ACK

| Field | Bits | Meaning |
|---|---|---|
| ver | 2 | 0 |
| type | 3 | 1 |
| sender | 28 | Acknowledging station |
| dest | 28 | Original sender |
| msg_id | 16 | Message being acknowledged |
| snr | 6 | Received SNR, two's complement, −32..+31 dB in 1 dB steps |
| reserved | 29 | MUST be 0 |
| crc | 16 | CRC-16 over the preceding 112 bits |

### 7.6 Short frame: beacon

| Field | Bits | Meaning |
|---|---|---|
| ver | 2 | 0 |
| type | 3 | 2 |
| sender | 28 | Beaconing station |
| grid | 15 | 4-character Maidenhead locator, FT8 packing; 32767 = none |
| group_tags | 24 | Two 12-bit truncated group IDs the station is listening to; 0 = none |
| status | 4 | 0 = listening, 1 = away, others reserved |
| reserved | 36 | MUST be 0 |
| crc | 16 | CRC-16 over the preceding 112 bits |

### 7.7 CRC

CRC-16/CCITT-FALSE: polynomial 0x1021, initial value 0xFFFF, no input or
output reflection, no final XOR, computed over the field bits in order,
MSB first. A block whose CRC fails MUST be discarded.

### 7.8 Worked airtime

Text at 2.5 bit/char plus about 6 bits of termination:

| Message | Payload bits | Blocks | Airtime F | Airtime S |
|---|---|---|---|---|
| 40 chars | ≈ 106 | 1 | 0.768 + 4.224 = 5.0 s | 3.84 + 21.12 = 25.0 s |
| 240 chars | ≈ 606 | 4 | 0.768 + 16.9 = 17.7 s | 3.84 + 84.5 = 88.3 s |
| ACK / beacon | 128 | 1 short | 0.768 + 2.176 = 2.9 s | 3.84 + 10.88 = 14.7 s |

## 8. Forward error correction

### 8.1 Encoding

Information bits are encoded with the systematic CCSDS TC code of Section 7.1.
The 512 (or 256) coded bits are interleaved (8.3) and then grouped into
4-bit data values.

### 8.2 Soft demodulation

For each data symbol the receiver computes the energy `e_t` in each of the 16
tone positions at the estimated `(τ, ν)`, undoes the hop offset to recover
energies per data value, and forms per-bit log-likelihood ratios. With
`γ̂` the estimated symbol SNR and `s_d = e_d · γ̂ / (1 + γ̂)`:

    L_b = max_{d : bit_b(d) = 0} s_d − max_{d : bit_b(d) = 1} s_d

A receiver MAY use the exact log-sum-exp form instead of max. LLRs are
passed to a belief-propagation decoder with at most 50 iterations. A block
is accepted only if the CRC verifies. On failure the receiver SHOULD retry
with up to 3 neighbouring `(τ, ν)` hypotheses.

A symbol known to be corrupted (for example blanked by successive
cancellation) is given all-zero LLRs.

### 8.3 Interleaver

The coded bits of a long block are written row by row into a 16 × 32 array
and read column by column before symbol mapping; a short block uses 16 × 16.
This spreads a run of lost symbols across the codeword. The same interleaver
is used in both profiles.

## 9. Content coding

### 9.1 Callsigns

Standard callsigns are packed into 28 bits exactly as in the FT8 protocol
(Franke, Somerville and Taylor, "The FT4 and FT8 Communication Protocols",
QEX July/August 2020, Section on standard callsign packing), giving values
below 262 177 560. Values from 262 177 560 upward are reserved: a callsign
that cannot be packed MUST be sent as 262 177 560 plus the low 22 bits of
the FNV-1a 32-bit hash of its uppercase ASCII text. Receivers display such a
sender as the hash in angle brackets until the full callsign is learned from
message text.

### 9.2 Groups

A group is named by the user as free text. Its on-air ID is the low 28 bits
of the FNV-1a 32-bit hash of the name in Unicode NFC form, lower-cased,
encoded as UTF-8. Beacons carry the low 12 bits of the same hash as a tag.

### 9.3 Message ID

`msg_id` is 16 random bits drawn by the sender per message. The pair
`(sender, msg_id)` identifies a message for de-duplication and
acknowledgement. Receivers MUST keep the pair for at least one hour.

### 9.4 Text payload

#### Alphabet

A 7-bit symbol alphabet of about 120 entries: ASCII 0x20–0x7E, newline, the
letters å ä ö Å Ä Ö é É ü Ü ø Ø æ Æ, the characters € – “ ” ’ …, plus three
control symbols ESC, REP and EOT. The exact table is normative and is
published as an appendix once the prior model (below) is frozen.

#### Coder

A 32-bit range coder with an order-1 adaptive model: one 128-entry frequency
table per preceding symbol, initialised from a static prior table that is
part of this specification, incremented by 24 per occurrence, and halved when
its total exceeds 4096. The halving bounds any symbol's cost to 12 bits.

Code points outside the alphabet are sent as ESC followed by a 21-bit code
point, coded as raw bits. REP re-emits the previously escaped code point, so
runs of the same emoji are cheap. Multi-code-point emoji sequences are
successive escapes.

The payload ends with EOT, then the coder is flushed with 2 bits, then the
block is padded with zeros.

#### Bound and fallback

The sender MUST also compute the raw UTF-8 byte length. If the coded payload
exceeds 8 bits per byte, the sender MUST set `raw_text` and send the UTF-8
bytes uncoded, followed by a zero byte. A payload MUST NOT exceed 1701 bits
in either form; the user interface SHOULD show the block count before
sending. "240 characters" is the typical limit for ordinary text, not a
guarantee for arbitrary content.

#### Prior model

The static prior is derived from an English and Swedish chat corpus. Until it
is published, implementations MUST use a uniform prior; this costs about one
bit per character and is interoperable as long as both ends use the same
table. The published table will be versioned by `ver`.

## 10. Receiver reference pipeline

1. Capture audio with cpal at 48 kHz mono and decimate by 4 with a polyphase
   FIR to 12 kHz. 12 000 / 31.25 = 384, so symbol periods are integer sample
   counts.
2. Energy matrix, profile F: rectangular window of 384 samples, zero-padded
   to 1536 points (7.8125 Hz bins, 4 per tone), hop 96 samples (8 ms). 125
   FFTs per second.
3. Energy matrix, profile S: window 1920, zero-padded to 3840 (3.125 Hz, 10
   per tone), hop 480 (40 ms). 25 FFTs per second.
4. Keep bins covering 300–2700 Hz. Ring buffers of 30 s (F) and 150 s (S),
   about 8 MB of f32 in total.
5. Per-bin baseline: running 30th percentile over 8 s (F) / 40 s (S), updated
   every 0.5 s. `Ẽ = E / baseline`, clipped at 20.
6. Preamble search as in Section 6 on every new column.
7. On detection: re-extract the burst's 12 kHz samples, mix by `−ν̂`, compute
   the 16 tone energies per symbol with Goertzel filters at `τ̂`, demodulate
   and decode block by block, tracking as in 6.5. Continue until `total`
   blocks are decoded, 8 blocks have elapsed, or the lane energy vanishes.
8. Successive cancellation (OPTIONAL): after decoding a burst, blank its 132
   tone positions per block in the energy matrix and re-run the search, so a
   weaker overlapping burst can be found.

Estimated load on one Raspberry Pi 4 core: about 8 MFLOP/s of FFT, under
1 M adds/s of correlation for 4 lanes × 2 profiles, and 2–3 ms per LDPC
decode. Under 5 % of one core. Decode latency about 0.3 s after block end in
profile F.

Reported SNR for the heard list is `10·log10(γ̂ · R_s / 2500)` dB with `γ̂`
from the preamble, so that figures are comparable with FT8 reports.

## 11. Transmitter reference pipeline

1. Build the tone index sequence: preamble, then blocks with pilots and
   hop-mapped data.
2. Synthesise at 48 kHz: convolve the index sequence with the Gaussian pulse
   of Section 3.2, map to instantaneous frequency `f(L, 0) + 31.25 · idx(t)`,
   integrate into a phase accumulator, output `A · sin(phase)` with constant
   `A` = −6 dBFS.
3. Apply a raised-cosine amplitude ramp of 32 ms at the start (first tone
   held) and at the end.
4. Keying sequence via rigctld: `T 1`, wait `tx_delay` (default 150 ms,
   configurable 50–500 ms), play audio, 50 ms of silence, `T 0`.
5. Set the rig to data mode with the widest available filter (`M PKTUSB
   3000`) and set the dial frequency with `F`.

Audio level: adjust so the rig's ALC meter shows at most the first segment.
Because the envelope is constant, ALC compression affects only the ramps.
The audio chain MUST NOT clip: the second harmonic of a lane 1 tone lands in
lane 3.

## 12. Channel access

A lane is **busy** if (a) a burst detected on it has not reached its
predicted end, or (b) the mean of `Ẽ` over the lane's 16 bins for the last
1 s exceeds 2.0 (3 dB above baseline).

Before transmitting a message or beacon a station MUST:

1. Choose the lane: the user's configured lane, or if set to automatic, the
   idle lane with the lowest mean `Ẽ` over the last 10 s.
2. If the lane is busy, wait for it to become idle, then back off a random
   0..7 slots of 0.5 s and re-check. Repeat until idle.

ACK bursts (Section 13) are sent without listen-before-talk.

A station SHOULD default to profile F and use S only when the heard-list SNR
of the destination, or the operator, says so. A 240-character message in S
holds a lane for 88 s.

## 13. Acknowledgement and retry

A message with `dest_type = 2` (callsign) and `ack_req` set requests an ACK.

- The addressed station, if it decoded block 0 and all blocks, MUST send an
  ACK short frame on the same lane and profile, starting 1.0 s (F) or 2.0 s
  (S) after the end of the message burst.
- Other stations that decoded block 0 of such a message MUST treat the lane
  as busy for the ACK window plus the ACK airtime plus 1 s.
- The sender waits for the ACK window. If no ACK arrives, it MAY retransmit
  the identical burst after a uniform random delay of 15–45 s (F) or 30–90 s
  (S), with listen-before-talk, up to 2 retransmissions (3 transmissions in
  total). The user interface SHOULD show sent, delivered and failed states.

Group and broadcast messages are never acknowledged.

## 14. Presence

Every decoded frame updates a heard list entry for its sender: callsign,
SNR, lane, profile, time, and for beacons the locator, group tags and status.

A station MAY send a beacon when it has neither transmitted nor been
addressed for at least 10 minutes, at uniform random intervals of 10–30
minutes thereafter. Beacons use profile F unless the operator chooses S.

## 15. Relay (reserved)

Version 0 stations MUST send `hop = 0` and MUST NOT relay. The following
rules are reserved for a future version and are given so that the frame
format need not change:

- A station relays a group or broadcast message once, after a random delay,
  if it has not heard the same `(sender, msg_id)` relayed, with `hop`
  incremented. `hop = 3` is never relayed.
- Receivers de-duplicate on `(sender, msg_id)` regardless of `hop`.
- ACKs are not relayed in the first relay version.

## 16. Regulatory notes

- The emission is 16-tone MFSK with a necessary bandwidth of 500 Hz, the
  same as Olivia 16/500. The per-symbol tone permutation changes which tone
  carries which value; it does not expand the bandwidth, so the emission is
  not spread spectrum under the usual definitions.
- IARU Region 1: fits the narrow-band digimode segments (500 Hz) on all HF
  bands; not the 200 Hz sub-segments of 80 m.
- FCC Part 97: well within the 2.8 kHz data bandwidth limit; the symbol rate
  is 31.25 Bd; this document constitutes the public specification required
  for an unspecified digital code.
- Confirmation of the spread-spectrum interpretation with PTS and the FCC is
  an open item (DESIGN.md Section 12).

## 17. Appendices (to be added)

- A. Costas sequences `C`, `C'`, PHASE templates and a worked correlation
  example.
- B. Text alphabet table and static prior frequencies.
- C. Test vectors: packed callsigns, group hashes, CRC, a complete encoded
  block, a complete tone sequence.
