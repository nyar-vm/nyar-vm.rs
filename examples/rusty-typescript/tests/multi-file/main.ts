import { add, greet } from './lib';

function main() {
    const sum = add(10, 20);
    print('Sum: ' + sum);

    const message = greet('NyarVM');
    print(message);
}

main();
