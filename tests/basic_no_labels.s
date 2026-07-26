push rbp
mov rbp, rsp
xor eax, eax          ; eax = 0 (sum accumulator, also zero-extends rax)
xor ecx, ecx          ; ecx = 0 (loop index)
cmp rcx, rsi
add eax, edi
inc rcx
pop rbp
ret