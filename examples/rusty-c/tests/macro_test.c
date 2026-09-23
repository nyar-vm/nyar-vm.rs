#define GREETING "Hello from macro!"
#define PI 3
#define SQUARE(x) ((x) * (x))

#ifdef GREETING
int main() {
    int radius = 5;
    int area = PI * SQUARE(radius);
    printf("%s Area is roughly %d\n", GREETING, area);
    return 0;
}
#else
#error "GREETING not defined"
#endif
