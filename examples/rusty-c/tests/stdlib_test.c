#include <stdio.h>
#include <math.h>
#include <stdlib.h>

int main() {
    printf("Testing standard library support...\n");

    // Math functions
    double s = sqrt(16.0);
    printf("sqrt(16.0) = %f\n", s);

    int a = abs(-10);
    printf("abs(-10) = %d\n", a);

    // Memory functions
    void* ptr = malloc(100);
    if (ptr != NULL) {
        printf("malloc(100) success: %p\n", ptr);
        free(ptr);
        printf("free success\n");
    }

    printf("Standard library test finished.\n");
    return 0;
}
