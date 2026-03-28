.globl _main
_main:
    movl $4, %eax
    pushq %rax
    movl $2, %eax
    pushq %rax
    movl $1, %eax
    movl %eax, %ecx
    popq %rax
    addl %ecx, %eax
    movl %eax, %ecx
    popq %rax
    imull %ecx, %eax
    ret