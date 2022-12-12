import { readFileSync, writeFileSync } from 'fs';

const p = new URL('./overload.rs', import.meta.url);
let s = readFileSync(p, 'utf8');
if (s.includes('name.as_str() == "format"')) {
  console.log('already patched');
  process.exit(0);
}
const needle = ') -> Option<HirResolvedCall> {\n    // Function-typed locals/params are indirect calls';
const i = s.indexOf(needle);
if (i < 0) {
  console.error('needle not found');
  process.exit(1);
}
const insert = `) -> Option<HirResolvedCall> {
    // Runtime format(...) stub (wasm/CLR emitters). Not a std overload.
    if let Some(name) = extract_callable_name(callee) {
        if name.as_str() == "format" {
            return Some(HirResolvedCall {
                symbol: NamePath::new(vec![Identifier::new("format")]),
                domain: HirCallableDomain::Function,
                return_type: ValkyrieType::Utf8,
                parameter_types: args.iter().map(|_| ValkyrieType::AutoType).collect(),
                extractor_payload_type: None,
                intrinsic_opcode: None,
            });
        }
    }
    // Function-typed locals/params are indirect calls`;
const out = s.slice(0, i) + insert + s.slice(i + needle.length);
writeFileSync(p, out);
console.log('patched ok');
