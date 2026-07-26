.intel_syntax noprefix
.global sum_array
 
sum_array:
    push rbp
    mov rbp, rsp
    xor eax, eax          ; eax = 0 (sum accumulator, also zero-extends rax)
    xor ecx, ecx          ; ecx = 0 (loop index)
.L1:
    cmp rcx, rsi
    ;jge .L2
    add eax, [rdi + rcx*4]
    inc rcx
    ;jmp .L1
.L2:
    pop rbp
    ret
 