import re

with open('index.html', 'r', encoding='utf-8') as f:
    html = f.read()

# Extract the style block
style_match = re.search(r'<style>(.*?)</style>', html, re.DOTALL)
if style_match:
    style_content = style_match.group(1)
    # Find all rules containing width
    rules = re.findall(r'([^{]+)\{([^}]+)\}', style_content, re.DOTALL)
    for selector, body in rules:
        selector = selector.strip()
        body = body.strip()
        if 'width' in body or 'max-width' in body:
            print(f"Selector: {selector}\nBody: {body}\n" + "-"*40)
else:
    print("No style block found!")
