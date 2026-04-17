import json

base_actions = {
    "Merge": ["git merge main", "gh pr merge 42"],
    "Deploy": ["git push", "docker push img", "helm upgrade app", "cargo publish"],
    "Apply": ["kubectl apply -f k.yaml", "terraform apply"]
}

variants = []
def add(cmd, action):
    variants.append((cmd, action))

# Add existing ones from corpus (32)
add("sudo git merge main", "Merge")
add("sudo /usr/bin/git merge main", "Merge")
add("env FOO=bar git merge main", "Merge")
add("env FOO=bar /usr/bin/git merge main", "Merge")
add("bash -c \"git merge main\"", "Merge")
add("bash -lc \"git merge main\"", "Merge")
add("bash -euxo pipefail -c \"git merge main\"", "Merge")
add("sh -c \"git merge main\"", "Merge")
add("xargs -I{} git merge main", "Merge")
add("git -C /repo merge main", "Merge")
add("git -c core.editor=vim merge main", "Merge")
add("gh pr merge 42", "Merge")
add("sudo gh pr merge 42", "Merge")
add("/usr/bin/gh pr merge 42", "Merge")
add("kubectl apply -f k8s.yaml", "Apply")
add("kubectl --context=prod apply -f k8s.yaml", "Apply")
add("sudo /usr/bin/kubectl -n prod apply -f k8s.yaml", "Apply")
add("terraform apply", "Apply")
add("terraform -chdir=infra apply", "Apply")
add("sudo terraform -chdir infra apply", "Apply")
add("helm upgrade --install app chart", "Deploy")
add("sudo helm upgrade --install app chart", "Deploy")
add("docker push ghcr.io/org/image:latest", "Deploy")
add("sudo /usr/bin/docker push ghcr.io/org/image:latest", "Deploy")
add("cargo publish --dry-run", "Deploy")
add("sudo cargo publish --dry-run", "Deploy")
add("env FOO=bar docker push ghcr.io/org/image:latest", "Deploy")
add("env FOO=bar cargo publish --dry-run", "Deploy")
add("bash -lc \"docker push ghcr.io/org/image:latest\"", "Deploy")
add("bash -lc \"cargo publish --dry-run\"", "Deploy")
add("xargs -n 1 docker push ghcr.io/org/image:latest", "Deploy")
add("xargs -n1 cargo publish --dry-run", "Deploy")

# Wrappers
wrappers = ["nohup", "timeout 10", "stdbuf -oL", "watch -n 1", "time", "nice -n 10", "ionice -c 2", "env -i", "sudo -E", "sudo -u root", "doas", "su -c"]
for w in wrappers:
    add(f"{w} git merge main", "Merge")
    add(f"{w} docker push img", "Deploy")
    add(f"{w} terraform apply", "Apply")

# Eval
add("eval 'git merge main'", "Merge")
add("eval \"git push\"", "Deploy")
add("eval $(echo git merge main)", "Merge")
add("eval `echo git push`", "Deploy")

# $(cmd)
add("$(echo git) merge main", "Merge")
add("git $(echo merge) main", "Merge")
add("echo $(git merge main)", "Merge")
add("$(git merge main)", "Merge")
add("FOO=$(git merge main)", "Merge")
add("FOO=$(git push) bar", "Deploy")
add("$(which git) merge main", "Merge")
add("echo $(terraform apply)", "Apply")

# Backticks
add("`echo git` merge main", "Merge")
add("git `echo merge` main", "Merge")
add("echo `git merge main`", "Merge")
add("`git merge main`", "Merge")
add("FOO=`git push` bar", "Deploy")
add("`which git` merge main", "Merge")
add("echo `kubectl apply -f k.yaml`", "Apply")

# Here-docs
add("bash <<EOF\ngit merge main\nEOF", "Merge")
add("sh -c 'git merge main' <<EOF\nEOF", "Merge")
add("cat <<EOF | sh\ngit merge main\nEOF", "Merge")
add("cat <<'EOF' | bash\ngit merge main\nEOF", "Merge")
add("zsh <<EOF\ngit push\nEOF", "Deploy")

# Quirks & Shell operators
add("git merge main &|", "Merge") # zsh
add("git merge main > /dev/null", "Merge")
add("git merge main &> /dev/null", "Merge")
add("env FOO=bar; git merge main", "Merge")
add("git merge main; echo done", "Merge")
add("git merge main && echo done", "Merge")
add("echo start || git merge main", "Merge")
add("git merge main | tee out.log", "Merge")
add("{ git merge main; }", "Merge")
add("(git merge main)", "Merge")

# Xargs
add("echo main | xargs git merge", "Merge")
add("echo main | xargs -t git merge", "Merge")
add("echo main | xargs -I{} git merge {}", "Merge")
add("find . -name '*.txt' | xargs git push", "Deploy")
add("xargs -0 git merge < file", "Merge")
add("xargs --null git merge < file", "Merge")
add("xargs -P 4 git push", "Deploy")

# Find -exec
add("find . -exec git merge main \\;", "Merge")
add("find . -exec git push origin main {} +", "Deploy")
add("find . -type f -execdir git merge main \\;", "Merge")
add("find . -name '*.yaml' -exec kubectl apply -f {} \\;", "Apply")

# Git aliases and flags
add("git -c alias.m=merge m main", "Merge")
add("git --git-dir=.git merge main", "Merge")
add("git --work-tree=. merge main", "Merge")
add("git -C /tmp merge main", "Merge")
add("git --namespace=foo merge main", "Merge")

# Kubectl plugin / flags
add("kubectl plugin apply -f foo.yaml", "Apply")
add("kubectl kustomize apply", "Apply")
add("kubectl --server=foo apply -f bar", "Apply")
add("kubectl --token=foo apply -f bar", "Apply")

# Terraform
add("terraform --chdir=infra apply", "Apply")

# Quotes and escapes
add("g\\i\\t m\\e\\r\\g\\e main", "Merge")
add("\"g\"i\"t\" 'm'e'r'g'e' main", "Merge")
add("git m\"e\"rge main", "Merge")
add("git m\\erge main", "Merge")
add("g''i\"\"t merge main", "Merge")

# Combinations
add("sudo nohup timeout 10 git merge main", "Merge")
add("env FOO=bar xargs -I{} sudo git merge {}", "Merge")
add("bash -c \"sudo git merge main\"", "Merge")
add("sh -c 'nohup git merge main'", "Merge")
add("eval \"$(echo sudo git merge main)\"", "Merge")

seen = set()
unique_variants = []
for cmd, action in variants:
    if cmd not in seen:
        seen.add(cmd)
        unique_variants.append((cmd, action))

for cmd, action in unique_variants:
    print(f'        (r#"{cmd}"#, PolicyGateAction::{action}),')

print(f"Total: {len(unique_variants)}")
