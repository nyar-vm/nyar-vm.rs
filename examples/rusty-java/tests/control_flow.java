public class ControlFlow {
    public static void main(String[] args) {
        // If-Else
        int x = 10;
        if (x > 5) {
            System.out.println("x is greater than 5");
        } else {
            System.out.println("x is not greater than 5");
        }

        // While loop
        int i = 0;
        while (i < 5) {
            System.out.println(i);
            i = i + 1;
        }

        // Do-While loop
        int j = 0;
        do {
            System.out.println(j);
            j = j + 1;
        } while (j < 3);

        // For loop
        for (int k = 0; k < 5; k = k + 1) {
            if (k == 2) {
                continue;
            }
            if (k == 4) {
                break;
            }
            System.out.println(k);
        }

        // Switch
        int day = 2;
        switch (day) {
            case 1:
                System.out.println("Monday");
                break;
            case 2:
                System.out.println("Tuesday");
                break;
            default:
                System.out.println("Other day");
        }
    }
}
