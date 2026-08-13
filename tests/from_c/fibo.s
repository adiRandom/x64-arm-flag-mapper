.LC0:
        .string "%d\n"
.LC1:
        .string "F(%d) = %d + %d = %d\n"
"fibonacci":
        push    rbp
        mov     rbp, rsp
        sub     rsp, 32
        mov     DWORD PTR [rbp-20], edi
        mov     eax, DWORD PTR [rbp-20]
        mov     esi, eax
        mov     edi, OFFSET FLAT:.LC0
        mov     eax, 0
        call    "printf"
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
        mov     DWORD PTR [rbp-4], eax
        mov     eax, DWORD PTR [rbp-20]
        sub     eax, 2
        mov     edi, eax
        call    "fibonacci"
        mov     DWORD PTR [rbp-8], eax
        mov     edx, DWORD PTR [rbp-4]
        mov     eax, DWORD PTR [rbp-8]
        add     eax, edx
        mov     DWORD PTR [rbp-12], eax
        mov     esi, DWORD PTR [rbp-12]
        mov     ecx, DWORD PTR [rbp-8]
        mov     edx, DWORD PTR [rbp-4]
        mov     eax, DWORD PTR [rbp-20]
        mov     r8d, esi
        mov     esi, eax
        mov     edi, OFFSET FLAT:.LC1
        mov     eax, 0
        call    "printf"
        mov     eax, DWORD PTR [rbp-12]
        cdqe
.L3:
        leave
        ret
.LC2:
        .string "Enter a non-negative integer (n): "
.LC3:
        .string "%d"
.LC4:
        .string "Error: Please enter a valid non-negative integer."
.LC5:
        .string "Fibonacci term F(%d) = %lld\n"
"main":
        push    rbp
        mov     rbp, rsp
        sub     rsp, 16
        mov     edi, OFFSET FLAT:.LC2
        mov     eax, 0
        call    "printf"
        lea     rax, [rbp-12]
        mov     rsi, rax
        mov     edi, OFFSET FLAT:.LC3
        mov     eax, 0
        call    __isoc23_scanf
        cmp     eax, 1
        jne     .L6
        mov     eax, DWORD PTR [rbp-12]
        test    eax, eax
        jns     .L7
.L6:
        mov     edi, OFFSET FLAT:.LC4
        call    "puts"
        mov     eax, 1
        jmp     .L9
.L7:
        mov     eax, DWORD PTR [rbp-12]
        mov     esi, eax
        mov     edi, OFFSET FLAT:.LC0
        mov     eax, 0
        call    "printf"
        mov     eax, DWORD PTR [rbp-12]
        mov     edi, eax
        call    "fibonacci"
        mov     QWORD PTR [rbp-8], rax
        mov     eax, DWORD PTR [rbp-12]
        mov     rdx, QWORD PTR [rbp-8]
        mov     esi, eax
        mov     edi, OFFSET FLAT:.LC5
        mov     eax, 0
        call    "printf"
        mov     eax, 0
.L9:
        leave
        ret