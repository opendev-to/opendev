<!--
name: 'Tool Description: notebook_edit'
description: Edit cells in a Jupyter notebook
version: 2.0.0
-->

Edit cells in a Jupyter notebook (.ipynb file). Jupyter notebooks are interactive documents that combine code, text, and visualizations, commonly used for data analysis and scientific computing.

## Usage notes

- Supports three edit modes:
  - **replace**: Replace the content of an existing cell (default)
  - **insert**: Add a new cell at the specified position. Requires cell_type (code or markdown)
  - **delete**: Remove the cell at the specified position
- Cells can be identified by cell_id or cell_number. Cell numbering is 0-indexed
- The notebook_path must be an absolute path, not a relative path
- For insert mode, the new cell is inserted after the specified cell, or at the beginning if not specified
