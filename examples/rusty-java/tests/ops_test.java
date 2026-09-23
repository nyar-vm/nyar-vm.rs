public class OpsTest {
    public static void main(String[] args) {
        int a = 10;
        int b = 20;
        int c = a + b;
        int d = a - b;
        int e = a * b;
        int f = b / a;
        int g = b % a;
        
        boolean b1 = a == b;
        boolean b2 = a != b;
        boolean b3 = a < b;
        boolean b4 = a <= b;
        boolean b5 = a > b;
        boolean b6 = a >= b;
        
        boolean b7 = b1 && b2;
        boolean b8 = b1 || b2;
        boolean b9 = !b1;
        
        int h = a & b;
        int i = a | b;
        int j = a ^ b;
        int k = ~a;
        
        int l = a << 2;
        int m = a >> 1;
        int n = a >>> 1;
        
        a += 5;
        a -= 5;
        a *= 2;
        a /= 2;
        a %= 3;
        a &= 15;
        a |= 0;
        a ^= 0;
        a <<= 1;
        a >>= 1;
        a >>>= 1;
        
        int p = ++a;
        int q = a++;
        int r = --a;
        int s = a--;
        
        String str = "hello";
        boolean b10 = str instanceof String;
        
        int max = (a > b) ? a : b;
    }
}
