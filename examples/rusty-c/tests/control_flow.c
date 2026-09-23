
int main() {
    int i = 0;
    int sum = 0;

    // Test while loop with break and continue
    while (i < 10) {
        i = i + 1;
        if (i == 5) {
            continue;
        }
        if (i == 8) {
            break;
        }
        sum = sum + i;
    }
    // sum = 1 + 2 + 3 + 4 + 6 + 7 = 23
    print(sum); 
    print(1010101); // separator

    // Test for loop
    int for_sum = 0;
    for (int j = 0; j < 5; j = j + 1) {
        for_sum = for_sum + j;
    }
    // for_sum = 0 + 1 + 2 + 3 + 4 = 10
    print(for_sum);
    print(1010102); // separator

    // Test do-while
    int k = 0;
    int do_sum = 0;
    do {
        do_sum = do_sum + k;
        k = k + 1;
    } while (k < 3);
    // do_sum = 0 + 1 + 2 = 3
    print(do_sum);
    print(1010103); // separator

    // Test switch-case
    int x = 2;
    int res = 0;
    switch (x) {
        case 1:
            res = 10;
            break;
        case 2:
            res = 20;
            // fallthrough test
        case 3:
            res = res + 5;
            break;
        default:
            res = 100;
    }
    // res should be 20 + 5 = 25
    print(res);
    print(1010104); // separator

    // Test goto and label
    int g = 0;
    start_label:
    g = g + 1;
    if (g < 5) {
        goto start_label;
    }
    print(g); // should be 5
    
    return 0;
}
