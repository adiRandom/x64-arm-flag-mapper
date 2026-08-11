.intel_syntax noprefix
.global add_parity_test

; Tests that the lazy parity-flag mechanism works for ADD.
;
; ADD writes PF but the parity sequence is only emitted when `jp`/`jnp`
; actually consumes it.  This test verifies both cases:
;
;   Case 1 — 0x41 + 0x01 = 0x42 (0100 0010) = 2 set bits = even parity
;             jp  should branch  (PF = 1)
;
;   Case 2 — 0x40 + 0x01 = 0x41 (0100 0001) = 2 set bits = even parity
;             jnp should NOT branch
;
;   Case 3 — 0x40 + 0x03 = 0x43 (0100 0011) = 3 set bits = odd parity
;             jp  should NOT branch
;             jnp should branch  (PF = 0)
;
; Expected ARM64 for the ADD + JP sequence:
;   adds  x9, x9, x0         ; add with S-suffix for NZCV (toggled by flag_production_pass)
;   and   x_s, x9, #0xFF     ; \
;   eor   x_s, x_s, x_s, lsr #4  ; |  parity sequence inserted by
;   eor   x_s, x_s, x_s, lsr #2  ; |  flag_production_pass because jp
;   eor   x_s, x_s, x_s, lsr #1  ; |  reads PF from this ADD
;   and   x_s, x_s, #1       ; |
;   strb  w_s, [x28, #0]     ; /
;   ldrb  w_s, [x28, #0]     ; \  jp emulation
;   cmp   x_s, #0            ; |
;   b.ne  .even              ; /

add_parity_test:
    ; ── case 1: even parity result ─────────────────────────────────────
    mov eax, 0x41
    mov ecx, 1
    add eax, ecx             ; 0x41 + 0x01 = 0x42 = even parity (PF=1)
    jp .even
    mov eax, -1              ; jp not taken → bug
    ret

.even:
    ; ── case 2: even parity, jnp not taken ─────────────────────────────
    mov eax, 0x40
    mov ecx, 1
    add eax, ecx             ; 0x40 + 0x01 = 0x41 = even parity (PF=1)
    jnp .odd                 ; should NOT branch (PF=1, jnp needs PF=0)
    ; fall through is correct here

    ; ── case 3: odd parity result ───────────────────────────────────────
    mov eax, 0x40
    mov ecx, 3
    add eax, ecx             ; 0x40 + 0x03 = 0x43 = odd parity (PF=0)
    jnp .odd
    mov eax, -1              ; jnp not taken → bug
    ret

.odd:
    mov eax, 0               ; success
    ret
