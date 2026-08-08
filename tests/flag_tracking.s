.intel_syntax noprefix
.global flag_test

; Flag-tracking test.  Expected ARM64 output for each instruction:
;
;  add eax, ecx  (inside loop)  ->  add    (its ZF is overwritten by dec
;                                           before jnz reads it, so the
;                                           last writer of ZF at jnz is dec,
;                                           not this add)
;
;  dec ecx                      ->  subs   (ZF read directly by jnz)
;
;  add eax, ebx  (after loop)   ->  adds   (ZF read directly by jne, no
;                                           instruction overwrites it first)
;
;  cmp eax, 15                  ->  cmp    (cmp already sets NZCV in ARM64;
;                                           no S-suffix toggle needed)

flag_test:
    mov eax, 0
    mov ecx, 5

    ; Case 1 — dec feeds jnz, add's flags are irrelevant to the branch.
    ; last_flag_writer[ZF] is dec's index when jnz is processed, not add's.
.loop:
    add eax, ecx
    dec ecx
    jnz .loop

    ; Case 2 — add feeds jne with no intervening flag write.
    ; last_flag_writer[ZF] is this add's index when jne is processed.
    add eax, ebx
    jne .nonzero
    ret

.nonzero:
    ; Case 3 — cmp feeds jge.
    ; cmp/tst always set NZCV in ARM64; no toggle is needed.
    cmp eax, 15
    jge .large
    mov eax, -1
    ret

.large:
    ret
