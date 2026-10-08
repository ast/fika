# fika — design rationale

This document explains why the protocol in [SPEC.md](SPEC.md) looks the way it
does. The spec says what; this says why, what was considered instead, and what
is still open. Nothing here is normative.

## 1. What fika is for

Existing HF text modes fall into two camps. FT8 and its relatives are
extraordinarily sensitive but carry fixed 77-bit messages in 15 second slots;
a contact is an exchange of reports, not a conversation. Keyboard-to-keyboard
modes such as RTTY, PSK31, Olivia and JS8Call's live typing send characters as
you type them; they are personal, but they occupy a channel for as long as a
person is typing and leave a listener with half-finished lines.

fika takes the shape of a phone messaging app instead. A message is composed
off air, compressed, coded, and sent as one burst. Receivers show it once it
has decoded. Several stations share the same audio passband, and a listener
decodes all of them. The experience is a group chat, with the latency of HF.

The operating assumption throughout is "tactical": two radios and nothing
else. No internet, no NTP, no GPS, no coordinator station. Everything a
receiver needs to decode a burst is in the burst.

## 2. Asynchronous bursts rather than time slots

FT8 and JS8Call align every transmission to a clock. That buys two things. A
receiver knows exactly where every frame starts, so decoding dozens of
simultaneous signals is a matter of one FFT per slot. And the slot grid is a
free channel-access discipline.

The price is a clock accurate to about a second at every station, which in
practice means NTP or GPS. Both are infrastructure, and GPS is the first thing
to disappear in the situations a tactical mode is for. fika therefore uses
asynchronous bursts with a strong preamble, the way Olivia, VARA and the
military STANAG 4538 burst waveforms do. The receiver runs a continuous
preamble search instead of waiting for a slot boundary. That costs CPU, which a
Pi 4 has, and it requires the preamble to be designed carefully, which Section 5
is about.

## 3. Non-coherent MFSK with Gaussian shaping

An SSB transceiver's transmit chain has ALC, a power amplifier of modest
linearity, and a user who will not read the manual about drive levels. A
constant-envelope signal with one tone on at a time is indifferent to all of
that. It also needs no carrier phase tracking at the receiver, which matters on
HF where multipath and ionospheric motion scramble phase on a timescale of
tenths of a second at low SNR.

OFDM with PSK or QAM subcarriers, as in VARA and ARDOP, gets several times the
bits per hertz. But its crest factor forces 6 to 10 dB of power back-off, its
coherent tracking loops lose lock exactly where fika wants to operate, and
decoding several overlapping OFDM signals in one channel is hard. It is the
right tool for point-to-point file transfer on a channel you own, which is not
this problem.

Gaussian pulse shaping of the tone transitions, as in FT8's GFSK, keeps the
spectrum inside the lane. The phase stays continuous across tone changes, so
there are no clicks and no splatter from abrupt frequency jumps.

## 4. The military comparison

The question "what would the military do" is worth answering because the
answer shaped fika's burst discipline and clarified what it is not.

MIL-STD-188-110 and STANAG 4539 define a serial-tone waveform: a single 1800 Hz
carrier, 8-PSK at 2400 symbols per second, filling the 3 kHz channel, with
known probe symbols interleaved so an adaptive equalizer can undo multipath in
real time. Rates run from 75 bps (Walsh-spread) to 2400 bps with 8-PSK and
beyond with QAM. STANAG 4538 (3G ALE) adds a burst layer: short asynchronous
bursts with strong preambles for link setup, acknowledgements and traffic,
listen-before-talk, and link-quality sounding. No external time reference.
MIL-STD-188-141 (2G ALE) is 8-FSK at 125 baud, essentially MFSK.

fika keeps the burst discipline: asynchronous bursts, strong preamble,
listen-before-talk, a heard list that tells you who is reachable. It does not
adopt the serial-tone PSK waveform for version one, because that waveform
fills the whole channel and so forces strict one-at-a-time use, and because the
equalizer plus carrier and symbol tracking is a much larger modem to build and
prove than MFSK. Serial-tone PSK gives ten to fifty times the throughput of a
500 Hz MFSK lane, and it is the natural second waveform to add behind the same
framing when a channel is yours and speed matters. The frame header reserves a
waveform field for it.

## 5. Interference and multiple access: tone hopping

A narrow MFSK signal on a fixed frequency has one weakness: anything that lands
inside its 50 to 500 Hz, be it a carrier, another station, or a burst of
noise, takes the whole message with it. FT8 lives with this because it has
dozens of 50 Hz slots to choose from and a 15 second retry is cheap. For a
message that took 30 seconds to send, it is not acceptable.

fika's answer is frequency hopping inside the lane. Every symbol is still one
tone from an alphabet of 16, but the mapping from data value to tone position
is permuted every symbol by a pseudo-random pattern. Equivalently, it is
16-tone MFSK with a time-varying tone mapping. Three things follow.

**Interference costs symbols, not messages.** A steady carrier occupies one of
16 positions and so collides with about one symbol in 16. A noise burst 100 Hz
wide takes a fifth of the symbols for as long as it lasts. Rate one-half LDPC
with an interleaver absorbs both.

**Overlapping preambles separate, overlapping data does not.** Two stations
following the same pattern but started at different moments are at different
positions in it, so their preambles coincide on at most one symbol and the
receiver detects and times both. The data symbols are another matter, and the
first draft of this design over-claimed here. A second station of comparable
power puts a second energy peak into every overlapped symbol, and the receiver
cannot tell which peak is which. On average that erases two of the four coded
bits of the symbol, which is exactly the information rate of a rate one-half
code. Two equal-power bursts fully overlapped in one lane therefore both fail,
hopping or not. What does work: a partial overlap of up to about a fifth of a
block at equal power, and a full overlap when one station is 6 dB or more
stronger, where the strong one decodes and the weak one can be recovered by
successive cancellation. The practical multiple-access mechanism is one
station per lane plus listen-before-talk, with four lanes in a passband. A
rate one-quarter profile would ride through an equal-power overlap at half the
speed and is noted as a possible later addition.

**The hopping is not spread spectrum in the regulatory sense.** The necessary
bandwidth of the emission is 500 Hz, the same as Olivia 16/500, which also puts
16 tones across 500 Hz. The hop pattern changes which tone carries which value;
it does not widen the signal. This is also why the earlier idea of hopping a
single tone across the whole 2400 Hz passband was dropped: that emission
really is 2400 Hz wide, which keeps it out of the narrow-band segments where
digital operators actually gather.

### What the Costas array does and does not guarantee

The hop pattern is derived from a Costas array of order 16. A Costas array is a
permutation whose two-dimensional autocorrelation has at most one coincidence
for any non-zero shift in time and frequency. The first draft marked short
frames with the flipped sequence 15 − C, which is also Costas; the simulator
immediately produced ghost detections eight symbols off every short burst,
because for a Welch array 15 − C is C cyclically shifted by eight. Short
frames, the PHASE block and the pilots now use genuinely different Welch
arrays (primitive roots 6 and 7 against 3), chosen so that no shift of one
has more than four coincidences with another. That property is what makes the
preamble a near-ideal synchronisation sequence: a receiver can find a burst's
start time and frequency offset with no ambiguity, and two overlapping
preambles with different offsets coincide on at most one symbol.

For the data symbols the guarantee is weaker and it is important to be honest
about it. The tone actually transmitted depends on the data value as well as
the pattern, so from the point of view of another station it is effectively
random. A same-tone coincidence, about one symbol in 16, is actually harmless,
since it adds energy to the correct bin. The damage comes from the other
fifteen, where the interferer's tone is a second peak. The pattern does not
orthogonalise data streams; it randomises the damage, turns a steady carrier
into scattered erasures instead of a stuck bit position, and makes successive
cancellation possible because a decoded burst's tones are fully known. The
Costas structure buys unambiguous sync; the LDPC code does the rest.

### Simultaneous key-up

If two stations key up within one symbol period of each other they are at the
same position in the pattern, their preambles coincide on every symbol, and
the receiver sees one burst where there are two. Listen-before-talk makes this
rare, and hidden stations that cannot hear each other make it not impossible.
Every transmission therefore starts at a randomly chosen phase in the hop
pattern, so that even simultaneous starts give separable preambles and a
decodable stronger burst.

Encoding that phase naively, as a cyclic shift of the single Costas sequence,
would confound it with timing: a burst started one symbol later looks the same
as a burst with the phase advanced by one. The preamble is therefore two
parts, a fixed 16-symbol SYNC that gives timing and frequency, followed by an
8-symbol PHASE block that identifies the phase once timing is known. Any two
phases differ in at least seven of the eight PHASE positions.

### Near-far and AGC

A strong local station overlapping a weak distant one in the same lane is the
case where hopping pays. The strong burst decodes with about a decibel of
penalty, because the weak one is just a little extra noise to it. The receiver
can then re-encode the strong burst, blank its known tone positions in the
energy matrix, and search again, at which point the weak burst's preamble and
data are clean. This successive cancellation is specified as optional and is
the only way a weak station gets through an overlap. Each step of a chain needs
about 6 dB of separation. The other limit is the rig's AGC: a strong signal
turns the gain down for everything in the passband. As long as the sound card's
dynamic range keeps the weak signal above its own noise floor the SNR is
unchanged, and slow AGC or manual RF gain is the operator's job, as with every
digital mode.

## 6. Why 500 Hz and 16 tones

The IARU Region 1 band plan limits the narrow-band digimode segments to 500 Hz
of bandwidth (200 Hz in parts of 80 m). Those segments, on every band, are
where FT8, PSK, RTTY, Olivia and JS8 operate. The "all modes" segments allow
2700 Hz but are SSB phone territory, with only small slices set aside for wide
digital modes. A mode that needs 2400 Hz is confined to those slices.

So the lane is 500 Hz. The tone count follows from the symbol rate: for
orthogonal non-coherent detection, tone spacing must be at least the symbol
rate, so 16 tones at 31.25 Hz give 31.25 baud and 4 bits per symbol. 32 tones at
15.6 Hz would be under a decibel more sensitive but half the speed and would
need frequency accuracy of a few hertz, back towards FT8's fussiness. 8 tones
at 62.5 Hz would be faster per symbol but about a decibel worse per bit.

Against a 2400 Hz, 64-tone design the lane gives up about 1 to 1.5 dB of
sensitivity at the same bit rate, half the per-sender speed, and some fade
diversity, because HF selective fades are typically a few hundred hertz to a
kilohertz wide and can cover a 500 Hz lane whole. The interleaver spreads a
message over several seconds, which is where most of the fading protection
comes from anyway.

What the lane gives back is the lane structure itself. Four lanes fit side by
side in an SSB passband (five would need 2500 Hz plus guards, more than the
2400 Hz between 300 and 2700 Hz), a receiver decodes all four from the same
FFT at no extra cost, a sender picks the quietest by listening first, and a
wide interferer takes out one lane rather than everything. In a narrow segment
you run one lane. Same modem, same code. The lane pitch of 562.5 Hz leaves
62.5 Hz guard bins, which absorb the Gaussian skirts and up to 31 Hz of rig
frequency error without touching the neighbour.

### Why more tones beat fewer at the same bandwidth

FT8 uses 8 tones because 8 tones at 6.25 Hz is what fits in 50 Hz. With
non-coherent MFSK every symbol is one burst of energy, whatever the alphabet
size. More tones means more bits per burst, so for the same message in the
same time the symbols are longer and carry more energy each. The detector gives
back about a decibel for picking one of 16 rather than one of 8, and the net
is a gain. Wider spacing also tolerates more frequency error: 31.25 Hz spacing
is comfortable with 10 Hz of residual offset and a few hertz of Doppler
spread, where FT8's 6.25 Hz needs sub-hertz accuracy.

## 7. Two profiles

Local and good-path chat wants speed; a weak signal wants sensitivity. Since
sensitivity in non-coherent MFSK is set by energy per symbol, the cleanest way
to trade is symbol length alone. The fast profile uses 32 ms symbols and the
slow profile uses 160 ms symbols, the same as FT8. The derived thresholds are
about −12.5 dB and −19.5 dB in the usual 2500 Hz reference, against FT8's
−21 dB: FT8 gives up a little rate (1.57 information bits per symbol against
fika's 2) to buy that last 1.5 dB. Olivia 16/500, which has the same tones and
baud as the fast profile, sits near −13 dB, so the fast profile is at best
half a decibel ahead of it; the gain over Olivia is in the hopping, the framing
and the compression, not in raw sensitivity. Tones, hop pattern, framing and
FEC are identical between profiles. A receiver runs both detectors on every
lane and tells them apart by symbol rate alone, since each detector sees the
other profile's preamble as a smeared or repeated pattern that scores poorly.
A third, intermediate profile was considered and dropped to keep the spec
small; the header reserves room for it.

One consequence worth stating: a 240-character message in the slow profile
holds a lane for 88 seconds. The client defaults to the fast profile and
should suggest slow only when the heard-list SNR of the destination calls for
it.

## 8. LDPC

LDPC with soft-decision belief propagation is within about a decibel of the
theoretical limit at block sizes of a few hundred bits, and it takes per-bit
log-likelihood ratios as input, which fall out of the 16 tone energies
naturally. An erased symbol, from a collision or a carrier, simply contributes
flat likelihoods and the code works around it. FT8's 174/91 code is the proof
that this works on HF at scale.

The block is the CCSDS telecommand (512,256) code for messages and (256,128)
for acknowledgements and beacons. A 512-bit codeword is 128 symbols, which
with 4 pilots is 4.2 seconds in the fast profile: a reasonable granularity
for a message of one to eight blocks, and large enough that the code works
near its asymptotic performance. Putting the full 86-bit header in every block
would cost a third of each one, so only block 0 carries it and continuation
blocks carry a 35-bit header of sequence number and message ID. The price is
that a continuation block decoded without its block 0 has no sender.

Polar codes with list decoding are marginally better below 256 bits and are
the 5G choice, but there is no mature Rust implementation and list decoding is
more work to get right. Convolutional coding with Viterbi is simple and 1 to
2 dB worse. The `labrador-ldpc` crate ships the CCSDS telecommand codes with a
belief-propagation decoder, which is enough to start. A custom quasi-cyclic
code sized to fika's block is an open item.

## 9. Message model and text coding

A message is composed off air and sent as one burst. The cap of about 240
characters is a deliberate limit on airtime per message: under a minute on the
slow profile, tens of seconds on the fast one. Longer thoughts become two
messages, as they do in any messaging app.

Text is UTF-8, so Swedish letters and emoji work, compressed with an adaptive
arithmetic coder driven by a small fixed context model trained on chat-like
text. Ordinary English or Swedish lands near 2.5 bits per character, against 7
for ASCII or 6 for an FT8-style restricted alphabet. Callsigns inside the text
and a table of common phrases get shortcuts. The worst case, random text or a
string of emoji, must be bounded so a hostile message cannot expand without
limit; the spec gives the bound.

## 10. Groups, receipts, relay, presence

**Groups.** The WhatsApp analogy wants rooms. A message carries the sender's
callsign and a destination that is a named group, a single callsign, or
everyone. Group names are hashed to a short ID on air; you join a group by
typing its name, and your client shows only groups you are in. A single shared
room per frequency was considered and rejected because it gives up private
side chats and quiet nets.

**Receipts.** A direct message gets an automatic short acknowledgement burst
from the recipient, a tick in the sender's UI, and a few retries with backoff
if nothing comes back. Group messages are fire-and-forget: ten listeners would
mean ten acknowledgements, and acknowledgement storms would swamp a lane.

**Relay.** Store-and-forward relay doubles reach on a net but also doubles
airtime, and needs careful duplicate suppression before it is safe. The frame
header carries a message ID and a hop count so that relay can be added without
a format change, and the spec describes the rules, but version one does not
relay.

**Presence.** Every decoded burst updates a heard list with callsign, SNR, lane
and time, which the UI shows as who is around. A station may send a tiny
beacon every 10 to 30 minutes when idle so a quiet net does not look empty.
Active two-way sounding in the ALE style gives better link information but
costs airtime and starts to feel like a network rather than a chat.

## 11. Rig control and software shape

Version one keys the transmitter through hamlib's `rigctld` over TCP. It is a
line protocol, so the client is pure Rust, and hamlib already drives both the
IC-705 and the FT-891. Native CI-V and CAT drivers through the `sidebridge`
crate in rust-radio are a later option.

Version one is a single CLI binary that owns audio, rig and protocol. A daemon
serving a chat page to a phone over Wi-Fi is the obvious next shape, and the
civlink crate in rust-radio already does the serve-a-browser pattern, but it
does not block the modem.

## 12. Open issues

- **Custom LDPC code.** Design a quasi-cyclic code whose block matches fika's
  frame, rather than padding into a CCSDS size.
- **Near-far.** Characterise how much a strong overlapping station costs a weak
  one on real sound cards and rigs; decide whether successive interference
  cancellation is worth implementing.
- **Relay rules.** Random delay, hop limit, suppression window, and whether
  acknowledgements travel back along the relay path.
- **Regulatory confirmation.** Confirm with PTS and the FCC that 16-tone MFSK
  with a time-varying tone mapping is treated as MFSK and not as spread
  spectrum. The emission bandwidth argument is strong; it has not been tested.
- **Wide profile.** Parameters for a serial-tone PSK profile in the
  wide-digimode segments, behind the same framing.
- **Sensitivity.** Measured on AWGN with `fika sim`: 50 % decode at about
  −11.5 dB fast and −18.7 dB slow, 1 dB behind theory. Candidates for the
  missing decibel: normalised rather than plain min-sum decoding, the energy
  the Gaussian transitions take out of the measured bin in the fast profile,
  and the frame-grid frequency estimate. Under CCIR moderate fading the fast
  profile needs about −7 dB and under slow selective fading about −6 dB,
  because a 500 Hz lane can sit in a two-path notch for a whole burst.
- **False detections.** Chance matches of data symbols in a strong burst's
  own lane still pass the detector at roughly one per burst. Each costs one
  failed LDPC decode and never outranks a real preamble, but a cleaner
  discriminator would be welcome.
- **Robust profile.** A rate one-quarter code, or repetition of the (512,256)
  block, would decode through an equal-power overlap and add about 3 dB of
  sensitivity at half the speed. Worth a profile slot if on-air experience
  shows overlaps are common.
- **Text prior table.** The static order-1 prior has to be trained on an
  English and Swedish chat corpus, frozen, and published as an appendix. Until
  then implementations use a uniform prior at about one bit per character
  extra.
- **Timing walk.** A 100 ppm sound-card clock error walks symbol timing by
  about 9 ms over an 88-second burst. Per-block tracking is mandatory in the
  spec; whether the simple bounded re-estimation is enough on real hardware
  needs measurement on the shack Pi.
