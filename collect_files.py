import os

def collect_contents():
    # File to save everything into
    output_filename = "all_contents.txt"
    # Extensions to ignore
    ignored_extensions = {'.csv', '.pkl', '.pyc', '.png', '.jpg', '.jpeg', '.gif', '.ico'}
    # Directories to skip
    ignored_dirs = {'.git', '__pycache__', 'venv', '.venv', 'node_modules', '.gemini'}

    with open(output_filename, "w", encoding="utf-8") as outfile:
        for root, dirs, files in os.walk("."):
            # Modify dirs in-place to skip ignored directories
            dirs[:] = [d for d in dirs if d not in ignored_dirs]

            for file in files:
                # Don't include the output file or this script itself
                if file == output_filename or file == os.path.basename(__file__):
                    continue

                # Skip csv and binary files
                if any(file.lower().endswith(ext) for ext in ignored_extensions):
                    continue

                file_path = os.path.join(root, file)
                # Get relative path for the header
                rel_path = os.path.relpath(file_path, ".")

                try:
                    with open(file_path, "r", encoding="utf-8", errors="ignore") as infile:
                        content = infile.read()
                        outfile.write(f"{rel_path}:\n{content}\n\n")
                except Exception as e:
                    print(f"Error reading {rel_path}: {e}")

    print(f"Done! All contents (excluding CSVs) have been copied to {output_filename}")

if __name__ == "__main__":
    collect_contents()
