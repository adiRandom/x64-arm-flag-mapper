.intel_syntax noprefix

.section .rodata
.LC0:
        .string "r"
.LC1:
        .string "Error opening file"
.LC2:
        .string "%d %d %d"
.LC3:
        .string "BFS Traversal: "
.LC4:
        .string "%d "
.LC5:
        .string "tree.txt"
.LC6:
        .string "Failed to load tree from %s.\n"

.text
.globl createQueue
createQueue:
        push    rbp
        mov     rbp, rsp
        sub     rsp, 16
        mov     edi, 8008
        call    malloc
        mov     QWORD PTR [rbp-8], rax
        mov     rax, QWORD PTR [rbp-8]
        mov     DWORD PTR [rax+8000], 0
        mov     rax, QWORD PTR [rbp-8]
        mov     DWORD PTR [rax+8004], 0
        mov     rax, QWORD PTR [rbp-8]
        leave
        ret

.globl isEmpty
isEmpty:
        push    rbp
        mov     rbp, rsp
        mov     QWORD PTR [rbp-8], rdi
        mov     rax, QWORD PTR [rbp-8]
        mov     edx, DWORD PTR [rax+8000]
        mov     rax, QWORD PTR [rbp-8]
        mov     eax, DWORD PTR [rax+8004]
        cmp     edx, eax
        sete    al
        movzx   eax, al
        pop     rbp
        ret

.globl enqueue
enqueue:
        push    rbp
        mov     rbp, rsp
        mov     QWORD PTR [rbp-8], rdi
        mov     QWORD PTR [rbp-16], rsi
        mov     rax, QWORD PTR [rbp-8]
        mov     eax, DWORD PTR [rax+8004]
        cmp     eax, 999
        jg      .L7
        mov     rax, QWORD PTR [rbp-8]
        mov     eax, DWORD PTR [rax+8004]
        lea     ecx, [rax+1]
        mov     rdx, QWORD PTR [rbp-8]
        mov     DWORD PTR [rdx+8004], ecx
        mov     rdx, QWORD PTR [rbp-8]
        cdqe
        mov     rcx, QWORD PTR [rbp-16]
        mov     QWORD PTR [rdx+rax*8], rcx
.L7:
        nop
        pop     rbp
        ret

.globl dequeue
dequeue:
        push    rbp
        mov     rbp, rsp
        sub     rsp, 8
        mov     QWORD PTR [rbp-8], rdi
        mov     rax, QWORD PTR [rbp-8]
        mov     rdi, rax
        call    isEmpty
        test    eax, eax
        je      .L9
        mov     eax, 0
        jmp     .L10
.L9:
        mov     rax, QWORD PTR [rbp-8]
        mov     eax, DWORD PTR [rax+8000]
        lea     ecx, [rax+1]
        mov     rdx, QWORD PTR [rbp-8]
        mov     DWORD PTR [rdx+8000], ecx
        mov     rdx, QWORD PTR [rbp-8]
        cdqe
        mov     rax, QWORD PTR [rdx+rax*8]
.L10:
        leave
        ret

.globl createNode
createNode:
        push    rbp
        mov     rbp, rsp
        sub     rsp, 32
        mov     DWORD PTR [rbp-20], edi
        mov     edi, 24
        call    malloc
        mov     QWORD PTR [rbp-8], rax
        mov     rax, QWORD PTR [rbp-8]
        mov     edx, DWORD PTR [rbp-20]
        mov     DWORD PTR [rax], edx
        mov     rax, QWORD PTR [rbp-8]
        mov     QWORD PTR [rax+8], 0
        mov     rax, QWORD PTR [rbp-8]
        mov     QWORD PTR [rax+16], 0
        mov     rax, QWORD PTR [rbp-8]
        leave
        ret

.globl loadTreeFromFile
loadTreeFromFile:
        push    rbp
        mov     rbp, rsp
        push    rbx
        sub     rsp, 8072
        mov     QWORD PTR [rbp-8072], rdi
        mov     rax, QWORD PTR [rbp-8072]
        lea     rsi, [rip + .LC0]
        mov     rdi, rax
        call    fopen
        mov     QWORD PTR [rbp-32], rax
        cmp     QWORD PTR [rbp-32], 0
        jne     .L14
        lea     rdi, [rip + .LC1]
        call    perror
        mov     eax, 0
        jmp     .L23
.L14:
        lea     rax, [rbp-8048]
        mov     edx, 8000
        mov     esi, 0
        mov     rdi, rax
        call    memset
        mov     QWORD PTR [rbp-24], 0
        jmp     .L16
.L22:
        mov     eax, DWORD PTR [rbp-8052]
        cdqe
        mov     rax, QWORD PTR [rbp-8048+rax*8]
        test    rax, rax
        jne     .L17
        mov     eax, DWORD PTR [rbp-8052]
        mov     ebx, DWORD PTR [rbp-8052]
        mov     edi, eax
        call    createNode
        movsxd  rdx, ebx
        mov     QWORD PTR [rbp-8048+rdx*8], rax
.L17:
        mov     eax, DWORD PTR [rbp-8052]
        cdqe
        mov     rax, QWORD PTR [rbp-8048+rax*8]
        mov     QWORD PTR [rbp-40], rax
        cmp     QWORD PTR [rbp-24], 0
        jne     .L18
        mov     rax, QWORD PTR [rbp-40]
        mov     QWORD PTR [rbp-24], rax
.L18:
        mov     eax, DWORD PTR [rbp-8056]
        cmp     eax, -1
        je      .L19
        mov     eax, DWORD PTR [rbp-8056]
        cdqe
        mov     rax, QWORD PTR [rbp-8048+rax*8]
        test    rax, rax
        jne     .L20
        mov     eax, DWORD PTR [rbp-8056]
        mov     ebx, DWORD PTR [rbp-8056]
        mov     edi, eax
        call    createNode
        movsxd  rdx, ebx
        mov     QWORD PTR [rbp-8048+rdx*8], rax
.L20:
        mov     eax, DWORD PTR [rbp-8056]
        cdqe
        mov     rdx, QWORD PTR [rbp-8048+rax*8]
        mov     rax, QWORD PTR [rbp-40]
        mov     QWORD PTR [rax+8], rdx
.L19:
        mov     eax, DWORD PTR [rbp-8060]
        cmp     eax, -1
        je      .L16
        mov     eax, DWORD PTR [rbp-8060]
        cdqe
        mov     rax, QWORD PTR [rbp-8048+rax*8]
        test    rax, rax
        jne     .L21
        mov     eax, DWORD PTR [rbp-8060]
        mov     ebx, DWORD PTR [rbp-8060]
        mov     edi, eax
        call    createNode
        movsxd  rdx, ebx
        mov     QWORD PTR [rbp-8048+rdx*8], rax
.L21:
        mov     eax, DWORD PTR [rbp-8060]
        cdqe
        mov     rdx, QWORD PTR [rbp-8048+rax*8]
        mov     rax, QWORD PTR [rbp-40]
        mov     QWORD PTR [rax+16], rdx
.L16:
        lea     rsi, [rbp-8060]
        lea     rcx, [rbp-8056]
        lea     rdx, [rbp-8052]
        mov     rax, QWORD PTR [rbp-32]
        mov     r8, rsi
        lea     rsi, [rip + .LC2]
        mov     rdi, rax
        mov     eax, 0
        call    __isoc23_fscanf
        cmp     eax, 3
        je      .L22
        mov     rax, QWORD PTR [rbp-32]
        mov     rdi, rax
        call    fclose
        mov     rax, QWORD PTR [rbp-24]
.L23:
        mov     rbx, QWORD PTR [rbp-8]
        leave
        ret

.globl bfs
bfs:
        push    rbp
        mov     rbp, rsp
        sub     rsp, 32
        mov     QWORD PTR [rbp-24], rdi
        cmp     QWORD PTR [rbp-24], 0
        je      .L30
        call    createQueue
        mov     QWORD PTR [rbp-8], rax
        mov     rdx, QWORD PTR [rbp-24]
        mov     rax, QWORD PTR [rbp-8]
        mov     rsi, rdx
        mov     rdi, rax
        call    enqueue
        lea     rdi, [rip + .LC3]
        mov     eax, 0
        call    printf
        jmp     .L27
.L29:
        mov     rax, QWORD PTR [rbp-8]
        mov     rdi, rax
        call    dequeue
        mov     QWORD PTR [rbp-16], rax
        mov     rax, QWORD PTR [rbp-16]
        mov     eax, DWORD PTR [rax]
        mov     esi, eax
        lea     rdi, [rip + .LC4]
        mov     eax, 0
        call    printf
        mov     rax, QWORD PTR [rbp-16]
        mov     rax, QWORD PTR [rax+8]
        test    rax, rax
        je      .L28
        mov     rax, QWORD PTR [rbp-16]
        mov     rdx, QWORD PTR [rax+8]
        mov     rax, QWORD PTR [rbp-8]
        mov     rsi, rdx
        mov     rdi, rax
        call    enqueue
.L28:
        mov     rax, QWORD PTR [rbp-16]
        mov     rax, QWORD PTR [rax+16]
        test    rax, rax
        je      .L27
        mov     rax, QWORD PTR [rbp-16]
        mov     rdx, QWORD PTR [rax+16]
        mov     rax, QWORD PTR [rbp-8]
        mov     rsi, rdx
        mov     rdi, rax
        call    enqueue
.L27:
        mov     rax, QWORD PTR [rbp-8]
        mov     rdi, rax
        call    isEmpty
        test    eax, eax
        je      .L29
        mov     edi, 10
        call    putchar
        mov     rax, QWORD PTR [rbp-8]
        mov     rdi, rax
        call    free
        jmp     .L24
.L30:
        nop
.L24:
        leave
        ret

.globl freeTree
freeTree:
        push    rbp
        mov     rbp, rsp
        sub     rsp, 16
        mov     QWORD PTR [rbp-8], rdi
        cmp     QWORD PTR [rbp-8], 0
        je      .L34
        mov     rax, QWORD PTR [rbp-8]
        mov     rax, QWORD PTR [rax+8]
        mov     rdi, rax
        call    freeTree
        mov     rax, QWORD PTR [rbp-8]
        mov     rax, QWORD PTR [rax+16]
        mov     rdi, rax
        call    freeTree
        mov     rax, QWORD PTR [rbp-8]
        mov     rdi, rax
        call    free
        jmp     .L31
.L34:
        nop
.L31:
        leave
        ret

.globl main
main:
        push    rbp
        mov     rbp, rsp
        sub     rsp, 16
        lea     rax, [rip + .LC5]
        mov     QWORD PTR [rbp-8], rax
        mov     rax, QWORD PTR [rbp-8]
        mov     rdi, rax
        call    loadTreeFromFile
        mov     QWORD PTR [rbp-16], rax
        cmp     QWORD PTR [rbp-16], 0
        je      .L36
        mov     rax, QWORD PTR [rbp-16]
        mov     rdi, rax
        call    bfs
        mov     rax, QWORD PTR [rbp-16]
        mov     rdi, rax
        call    freeTree
        jmp     .L37
.L36:
        mov     rax, QWORD PTR [rbp-8]
        mov     rsi, rax
        lea     rdi, [rip + .LC6]
        mov     eax, 0
        call    printf
.L37:
        mov     eax, 0
        leave
        ret
