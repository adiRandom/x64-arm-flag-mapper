.intel_syntax noprefix

.section .rodata
.LC0:
        .string "%d"

.text
.globl printLarger
printLarger:
        push    rbp
        mov     rbp, rsp
        sub     rsp, 16
        mov     DWORD PTR [rbp-4], edi
        mov     DWORD PTR [rbp-8], esi
        mov     edx, DWORD PTR [rbp-8]
        mov     eax, DWORD PTR [rbp-4]
        cmp     edx, eax
        cmovge  eax, edx
        mov     esi, eax
        lea     rdi, [rip + .LC0]
        mov     eax, 0
        call    printf
        nop
        leave
        ret

.globl main
main:
        push    rbp
        mov     rbp, rsp
        sub     rsp, 16
        mov     DWORD PTR [rbp-4], 2
        mov     DWORD PTR [rbp-8], 5
        mov     edx, DWORD PTR [rbp-8]
        mov     eax, DWORD PTR [rbp-4]
        mov     esi, edx
        mov     edi, eax
        call    printLarger
        nop
        leave
        ret
