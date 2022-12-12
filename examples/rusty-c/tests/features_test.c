typedef int my_int;

struct Point {
    int x;
    int y;
};

enum Color {
    RED,
    GREEN,
    BLUE
};

int main() {
    float f = 1.0;
    double d = 2.0;
    char c = 'a';
    char* s = "hello";
    
    my_int x = 10;
    
    struct Point p;
    p.x = 1;
    
    int a[10];
    a[0] = 1;
    
    int* ptr = &a[0];
    *ptr = 2;
    
    return 0;
}
