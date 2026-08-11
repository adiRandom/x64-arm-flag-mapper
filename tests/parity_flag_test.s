.intel_syntax noprefix
.global parity_test

; Test cases:
;
;   0x96 = 1001 0110  — 4 set bits — even parity (PF = 1)  →  jp taken
;   0x01 = 0000 0001  — 1 set bit  — odd  parity (PF = 0)  →  jnp taken
;
; Expected ARM64 output for `cmp eax, 0`:
;   cmp  x9, #0           ; existing CMP translation
;   sub  x12, x9, #0      ; recompute result for parity
;   and  x12, x12, #0xFF  ; isolate low byte
;   eor  x12, x12, x12, lsr #4
;   eor  x12, x12, x12, lsr #2
;   eor  x12, x12, x12, lsr #1
;   and  x12, x12, #1
;   strb w12, [x28, #0]   ; store parity flag
;
; Expected ARM64 output for `jp .even`:
;   ldrb w12, [x28, #0]
;   cmp  x12, #0
;   b.ne .even

parity_test:
    ; ── case 1: even parity (0x96 has 4 set bits) ──────────────────────────
    mov eax, 0x96
    cmp eax, 0              ; PF = parity(0x96) = 1 (even)
    jp .even_parity
    ; jp should have been taken; fall-through means a translation bug
    mov eax, -1
    ret

.even_parity:
    ; ── case 2: odd parity (0x01 has 1 set bit) ─────────────────────────────
    mov eax, 1
    cmp eax, 0              ; PF = parity(0x01) = 0 (odd)
    jnp .odd_parity
    ; jnp should have been taken; fall-through means a translation bug
    mov eax, -1
    ret

.odd_parity:
    mov eax, 0              ; success
    ret
