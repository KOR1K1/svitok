# SVITOK v1 - algorithm specification

This document is what you transcribe onto paper (3-4 A5 sheets by hand). It's
enough to write a compatible implementation from scratch, in any language, on
any CPU, with no internet, no libraries, and without this codebase. Every
operation is on 32-bit unsigned integers: addition mod 2^32, XOR, bitwise
rotation. Bytes pack into words little-endian (first byte is least significant),
except SHA-1 which is big-endian (see section 8).

If a new implementation reproduces the test vector at the bottom, it's compatible.

---

## 1. Overview

```
master key mk = KDF(seed from paper, phrase from your head)   - section 4
site password = Derive(mk, site, login, counter, policy)      - section 5
vault         = ChaCha20(mk) + MAC                            - section 7
paper form    = Base32 lines with check symbols               - section 6
```

Seed: 16 random bytes. It exists only on paper.

## 2. BLAKE2s-256 (hash; keyed = MAC/PRF)

State is 8 IV words (the fractional parts of the square roots of the first eight
primes 2,3,5,7,11,13,17,19 - same as SHA-256):

```
IV = 6A09E667 BB67AE85 3C6EF372 A54FF53A
     510E527F 9B05688C 1F83D9AB 5BE0CD19
```

Init: `h = IV; h[0] ^= 01010000 ^ (len(key)<<8) ^ 32`.
If there's a key (<=32 bytes): pad it with zeros to 64 bytes and feed it as the
first message block.

The message is split into 64-byte blocks (the last one zero-padded). Each block
goes through the compression function:

```
m[0..15] - the block as 16 LE words
v[0..7]=h; v[8..15]=IV; v[12]^=t_lo; v[13]^=t_hi;
if this is the last block: v[14] ^= FFFFFFFF
t - counter of bytes processed so far, including the current block; for the last
    block it's the real length WITHOUT the zero padding. Note: for a keyed hash of
    an empty message the only block IS the key block, so t = 64.

G(a,b,c,d,x,y):
  a+=b+x; d=(d^a)>>>16; c+=d; b=(b^c)>>>12;
  a+=b+y; d=(d^a)>>>8;  c+=d; b=(b^c)>>>7      (>>> = rotate right)

10 rounds; in round r the words are taken by the permutation SIGMA[r]:
  G(v0,v4,v8,v12, m[s0],m[s1])   G(v1,v5,v9,v13, m[s2],m[s3])
  G(v2,v6,v10,v14,m[s4],m[s5])   G(v3,v7,v11,v15,m[s6],m[s7])
  G(v0,v5,v10,v15,m[s8],m[s9])   G(v1,v6,v11,v12,m[s10],m[s11])
  G(v2,v7,v8,v13, m[s12],m[s13]) G(v3,v4,v9,v14, m[s14],m[s15])

SIGMA:
  0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15
  14 10 4 8 9 15 13 6 1 12 0 2 11 7 5 3
  11 8 12 0 5 2 15 13 10 14 3 6 7 1 9 4
  7 9 3 1 13 12 11 14 2 6 5 10 4 0 15 8
  9 0 5 7 2 4 10 15 14 1 11 12 6 8 3 13
  2 12 6 10 0 11 8 3 4 13 7 5 15 14 1 9
  12 5 1 15 14 13 4 10 0 7 6 3 9 2 8 11
  13 11 7 14 12 1 3 9 5 0 15 4 8 6 2 10
  6 15 14 9 11 3 0 8 12 2 13 7 1 4 10 5
  10 2 8 4 7 6 1 5 15 11 9 14 3 12 13 0

after the rounds: h[i] ^= v[i] ^ v[i+8]
```

Result: h as 32 LE bytes. Written below as `B2S(key, msg)`.
Check: `B2S(-, "abc") = 508c5e8c 327c14e2 e1a72ba3 4eeb452f
37458b20 9ed63a29 4d999b4c 86675982`.

## 3. ChaCha20 (stream cipher)

State is a 4x4 grid of words: constants || key (8 LE words) || counter (1) || nonce (3):

```
c0..c3 = 61707865 3320646E 79622D32 6B206574   ("expand 32-byte k")

QR(a,b,c,d):  a+=b; d=(d^a)<<<16;  c+=d; b=(b^c)<<<12;
              a+=b; d=(d^a)<<<8;   c+=d; b=(b^c)<<<7   (<<< = rotate left)

10 times: QR(0,4,8,12) QR(1,5,9,13) QR(2,6,10,14) QR(3,7,11,15)
          QR(0,5,10,15) QR(1,6,11,12) QR(2,7,8,13) QR(3,4,9,14)
then each word += its original value; output is 64 LE bytes.
```

Encryption = XOR the data with the stream of blocks (counter 0,1,2...).
Decryption is the same operation.

## 4. KDF: seed + phrase -> master key

Parameters written on the paper: `KDF M20 T21` (M = log2 of memory blocks,
T = log2 of iterations). The slowness and memory are what defend the phrase
against brute force if the paper is stolen. (Older papers may say M17; the
implementation reads whatever the paper says, so old and new both work.)

```
x = B2S(-, "SVITOK-KDF-v1" || le32(len seed) || seed || le32(len phrase) || phrase)
V[0] = x;  V[i] = B2S(-, V[i-1])   for i = 1 .. 2^M - 1   (32-byte blocks)
x = B2S(-, V[2^M - 1])
repeat 2^T times:
    j = le32(x[0..4]) mod 2^M
    x = B2S(-, x XOR V[j])
mk = B2S(-, "SVITOK-MK-v1" || x)
```

Fingerprint for the paper (catches a typo in the phrase):
`h = B2S(mk, "CTX:" || "fingerprint")`; two characters
`ALPH[h[0] mod 32], ALPH[h[1] mod 32]` (alphabet in section 6).

Subkeys: `subkey(mk, name) = B2S(mk, "CTX:" || name)`.

## 5. Site password

Input: site, login, counter v (all exact strings from your site list), and a
policy: length plus character classes. These five values are the *only* inputs.
A site list may carry extra bookkeeping next to an entry (an entry id, alias
domains for autofill matching, a display label) - none of that enters this
computation, so it can change freely without affecting any password. The classes
(order and contents are part of the algorithm - do not change them):

```
l: abcdefghijklmnopqrstuvwxyz      u: ABCDEFGHIJKLMNOPQRSTUVWXYZ
d: 0123456789                      s: !@#$%^&*()-_=+[]{};:,.?/
```

Length must be 1..128.

```
pk = subkey(mk, "password")
sk = B2S(pk, "PW:" || site || 1F || login || 1F || le32(v))
stream = ChaCha20(key=sk, nonce=0, counter from 0) - bytes in order

pick one of n choices (no bias):
    take a byte b until b < 256 - (256 mod n);  result is b mod n

alphabet = the allowed classes concatenated in order l,u,d,s
password[i] = alphabet[pick(len(alphabet))]  for i = 0 .. length-1

fill required classes (if length >= number of classes): while some class has no
representative - for each such class, in order l,u,d,s:
    pos = pick(length); password[pos] = class[pick(len(class))]
    then re-check every class (at most 32 passes)
```

**Custom symbols (policy extension).** A policy may restrict `s` to a subset of
the SYMBOLS set above (for sites that reject some specials). The subset must
contain only characters from SYMBOLS, with no duplicates. When present, that
subset replaces the full `s` class everywhere above (in the concatenated
alphabet and in the fill step), which changes the resulting password. If you
only ever use the four full classes, you can ignore this.

## 6. Paper encoding

Alphabet (Crockford Base32, no I L O U):

```
0 1 2 3 4 5 6 7 8 9 A B C D E F G H J K M N P Q R S T V W X Y Z
```

When reading: case doesn't matter, O->0, I->1, L->1, spaces and hyphens are
skipped.

Bytes -> symbols: bits most-significant first, 5 bits per symbol; the tail is
padded with zero bits (a strict decoder rejects a non-zero tail).

A paper line holds up to 16 data symbols (10 bytes); the last line may be
shorter:

```
NN XXXX XXXX XXXX XXXX K
NN - line number from 01;   K = ALPH[ B2S(-, "SVITOK-LINE-v1" || le32(NN) ||
                                the line's data symbols as ASCII, no spaces)[0] mod 32 ]
```

The check symbol K is computed over the actual data symbols on that line (16 for
a full line, fewer for the last one).

Final line: `== CCCC`, where CCCC is the first 4 bytes of
`B2S(-, "SVITOK-BLOB-v1" || all data bytes)`, each mod 32 -> a symbol.
**The checksum line is mandatory.** Reading a set of lines without it is rejected
(without it, a single-character typo the per-line check misses would slip through).

The seed (16 bytes) is encoded the same way: 2 lines + the checksum line.

## 7. Vault

Plaintext (varint = LEB128: 7 bits per byte, high bit means "more"):

```
01 || count || entries
entry:  tag(1 byte) || vlen(label) || label || payload
  1 password:  vlen || bytes
  2 TOTP:      flags || vlen || secret (raw bytes)
               flags: bits 0-1 algorithm (0=SHA1), bit 2: 8 digits,
                      bits 3-4 period (0->30s, 1->60s, 2->15s)
  3 codes:     encoding(0=utf8 joined by \n, 1=BCD) || vlen || data
               BCD: digit->nibble, A=code separator, F=padding
  4 note:      vlen || utf8
```

A strict reader rejects trailing bytes (must end exactly at the last entry) and
rejects any algorithm bits other than 0.

Envelope: `nonce(12 random bytes) || ciphertext || MAC(8 bytes)`

```
ek = subkey(mk, "vault-enc");  mak = subkey(mk, "vault-mac")
ciphertext = ChaCha20(ek, nonce, counter 0) XOR plaintext
MAC = B2S(mak, nonce || ciphertext)[0..8]      (encrypt-then-MAC; verify BEFORE decrypting, constant-time)
```

## 8. TOTP (for 2FA codes; only online sites need it)

RFC 6238 / 4226, HMAC-SHA1 (different crypto from the rest of Svitok - the sites
demand it). SHA-1: 5 BE words, 64-byte blocks, 80 rounds - see any reference;
check: `SHA1("abc") = a9993e36 4706816a ba3e2571 7850c26c 9cd0d89d`.

```
HMAC(k, m):
  if len(k) > 64: k = SHA1(k)
  pad k with zeros to 64
  SHA1((k ^ 5C..5C) || SHA1((k ^ 36..36) || m))
counter = unix time / period (BE, 8 bytes);  h = HMAC(secret, counter)
o = h[19] & 0F;  code = (h[o..o+4] BE & 7FFFFFFF) mod 10^digits
```

---

## Whole-system test vector

```
seed        = 00 11 22 33 44 55 66 77 88 99 AA BB CC DD EE FF
phrase      = "тайная фраза" (UTF-8), KDF M8 T10 (test parameters!)
mk          = 7c92b2aa fa7d6f2c 644f709b ab0b6b2f fd8329ed 5e12f663 13cf4c90 34c2625d
fingerprint = F5
password(site "mega.nz", login "me", v=1, len=20, cls=luds) = t^QeMQf0a#*Tl24(mC$?
```

If a new implementation produces these values, it's compatible.
