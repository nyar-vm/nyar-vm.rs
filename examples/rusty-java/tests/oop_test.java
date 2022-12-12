public interface Drawable {
    void draw();
}

public class Shape implements Drawable {
    private String name;
    
    public Shape(String name) {
        this.name = name;
    }
    
    public void draw() {
        System.out.println("Drawing shape: " + this.name);
    }
}

public class Circle extends Shape {
    private int radius;
    
    public Circle(String name, int radius) {
        super(name);
        this.radius = radius;
    }
    
    ↯Override
    public void draw() {
        System.out.print("Drawing circle: ");
        super.draw();
        System.out.println("With radius: " + this.radius);
    }
}

public class Main {
    public static void main(String[] args) {
        int x = 10;
        Circle c = new Circle("MyCircle", x);
        c.draw();
    }
}
