#include <stdio.h>

// Recursive function to calculate the nth Fibonacci number
long long fibonacci(int n) {    
    // Base cases
    if (n <= 0) {
        return 0;
    }
    if (n == 1) {
        return 1;
    }
    
    // Recursive call
    int recRes1 = fibonacci(n - 1);
    int recRes2 = fibonacci(n - 2);
    int result = recRes1 + recRes2;
    return result;
}

int main() {
    FILE *fp = fopen("fibo.txt", "r");
    if (fp == NULL) {
        printf("Error: could not open fibo.txt\n");
        return 1;
    }

    int n;
    while (fscanf(fp, "%d", &n) == 1) {
        if (n < 0) {
            printf("Skipping negative value %d\n", n);
            continue;
        }
        long long result = fibonacci(n);
        printf("Fibonacci term F(%d) = %lld\n", n, result);
    }

    fclose(fp);
    return 0;
}