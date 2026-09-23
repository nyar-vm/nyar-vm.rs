public class StdLibTest {
    public static void Main(string[] args) {
        double x = 1.0;
        double y = System.Math.Sin(x);
        System.Console.WriteLine(y);
        
        double z = Math.Sqrt(4.0);
        Console.WriteLine(z);
        
        string s = String.Concat("Hello", "World");
        Console.WriteLine(s);
        
        int i = Convert.ToInt32("123");
        Console.WriteLine(i);
        
        Environment.Exit(0);
    }
}
