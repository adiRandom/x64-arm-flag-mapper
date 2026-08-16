.intel_syntax noprefix

.section .rodata
.LC0:
        .string "r"
.LC1:
        .string "fibo.txt"
.LC2:
        .string "Error: could not open fibo.txt"
.LC3:
        .string "Skipping negative value %d\n"
.LC4:
        .string "Fibonacci term F(%d) = %lld\n"
.LC5:
        .string "%d"

.text
.globl fibonacci
fibonacci:
        push    rbp
        mov     rbp, rsp
        sub     rsp, 32
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
        call    fibonacci
        mov     DWORD PTR [rbp-4], eax
        mov     eax, DWORD PTR [rbp-20]
        sub     eax, 2
        mov     edi, eax
        call    fibonacci
        mov     DWORD PTR [rbp-8], eax
        mov     edx, DWORD PTR [rbp-4]
        mov     eax, DWORD PTR [rbp-8]
        add     eax, edx
        mov     DWORD PTR [rbp-12], eax
        mov     eax, DWORD PTR [rbp-12]
        cdqe
.L3:
        leave
        ret

.globl main
main:
        push    rbp
        mov     rbp, rsp
        sub     rsp, 32
        lea     rsi, [rip + .LC0]
        lea     rdi, [rip + .LC1]
        call    fopen
        mov     QWORD PTR [rbp-8], rax
        cmp     QWORD PTR [rbp-8], 0
        jne     .L8
        lea     rdi, [rip + .LC2]
        call    puts
        mov     eax, 1
        jmp     .L11
.L10:
        mov     eax, DWORD PTR [rbp-20]
        test    eax, eax
        jns     .L9
        mov     eax, DWORD PTR [rbp-20]
        mov     esi, eax
        lea     rdi, [rip + .LC3]
        mov     eax, 0
        call    printf
        jmp     .L8
.L9:
        mov     eax, DWORD PTR [rbp-20]
        mov     edi, eax
        call    fibonacci
        mov     QWORD PTR [rbp-16], rax
        mov     eax, DWORD PTR [rbp-20]
        mov     rdx, QWORD PTR [rbp-16]
        mov     esi, eax
        lea     rdi, [rip + .LC4]
        mov     eax, 0
        call    printf
.L8:
        lea     rdx, [rbp-20]
        mov     rax, QWORD PTR [rbp-8]
        lea     rsi, [rip + .LC5]
        mov     rdi, rax
        mov     eax, 0
        call    fscanf
        cmp     eax, 1
        je      .L10
        mov     rax, QWORD PTR [rbp-8]
        mov     rdi, rax
        call    fclose
        mov     eax, 0
.L11:
        leave
        ret
