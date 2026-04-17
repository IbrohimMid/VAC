import sys

with open("variants.rs") as f:
    lines = f.readlines()
variants_code = "".join(lines[:-1]) # remove the "Total" line

with open("crates/vac_core/tests/policy_gate.rs") as f:
    code = f.read()

start = code.find("let corpus = [\n")
end = code.find("    ];", start)

new_code = code[:start] + "let corpus = [\n" + variants_code + code[end:]

with open("crates/vac_core/tests/policy_gate.rs", "w") as f:
    f.write(new_code)
