"fibonacci":
        push    rbp
        mov     rbp, rsp
        push    rbx
        sub     rsp, 24
        mov     DWORD PTR [rbp-20], edi
        cmp     DWORD PTR [rbp-20], 0
        jg      .L2
        mov     eax, 0
        jmp     .L3
.L2:
        cmp     DWORD PTR [rbp-20], 1
        jne     .L4
        mov     eax, 1
        jmp     .L3
.L4:
        mov     eax, DWORD PTR [rbp-20]
        sub     eax, 1
        mov     edi, eax
        call    "fibonacci"
        mov     rbx, rax
        mov     eax, DWORD PTR [rbp-20]
        sub     eax, 2
        mov     edi, eax
        call    "fibonacci"
        add     rax, rbx
.L3:
        mov     rbx, QWORD PTR [rbp-8]
        leave
        ret
.LC0:
        .string "Enter a non-negative integer (n): "
.LC1:
        .string "%d"
.LC2:
        .string "Error: Please enter a valid non-negative integer."
.LC3:
        .string "Fibonacci term F(%d) = %lld\n"
"main":
        push    rbp
        mov     rbp, rsp
        sub     rsp, 16
        mov     edi, OFFSET FLAT:.LC0
        mov     eax, 0
        call    "printf"
        lea     rax, [rbp-12]
        mov     rsi, rax
        mov     edi, OFFSET FLAT:.LC1
        mov     eax, 0
        call    __isoc23_scanf
        cmp     eax, 1
        jne     .L6
        mov     eax, DWORD PTR [rbp-12]
        test    eax, eax
        jns     .L7
.L6:
        mov     edi, OFFSET FLAT:.LC2
        call    "puts"
        mov     eax, 1
        jmp     .L9
.L7:
        mov     eax, DWORD PTR [rbp-12]
        mov     edi, eax
        call    "fibonacci"
        mov     QWORD PTR [rbp-8], rax
        mov     eax, DWORD PTR [rbp-12]
        mov     rdx, QWORD PTR [rbp-8]
        mov     esi, eax
        mov     edi, OFFSET FLAT:.LC3
        mov     eax, 0
        call    "printf"
        mov     eax, 0
.L9:
        leave
        ret