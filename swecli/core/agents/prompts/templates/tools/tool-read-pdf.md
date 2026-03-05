<!--
name: 'Tool Description: read_pdf'
description: Extract text content from a PDF file
version: 2.0.0
-->

Extract text content from a PDF file. Returns full text with page markers, detected sections (Abstract, Introduction, Methods, etc.), and metadata (title, author, page count).

## Usage notes

- Best for reading research papers, academic publications, and technical documentation
- Automatically detects document structure and section boundaries
- For large PDFs, consider that the entire content will be returned — focus your analysis on the relevant sections
- Use this instead of read_file for PDF files, as read_file's PDF support is more limited
