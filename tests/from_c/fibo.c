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
    return fibonacci(n - 1) + fibonacci(n - 2);
}

int main() {
    int n;

    printf("Enter a non-negative integer (n): ");
    
    // Validate input
    if (scanf("%d", &n) != 1 || n < 0) {
        printf("Error: Please enter a valid non-negative integer.\n");
        return 1;
    }

    long long result = fibonacci(n);
    printf("Fibonacci term F(%d) = %lld\n", n, result);

    return 0;
}