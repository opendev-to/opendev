"""Tool for extracting text from PDF files, particularly academic papers."""

from __future__ import annotations

import re
from pathlib import Path
from typing import Any, Optional

from swecli.core.context_engineering.tools.implementations.base import BaseTool


class PDFTool(BaseTool):
    """Tool for extracting text content from PDF files.

    Designed for processing academic papers with support for:
    - Multi-column layouts
    - Section detection
    - Reference extraction
    """

    @property
    def name(self) -> str:
        """Tool name."""
        return "read_pdf"

    @property
    def description(self) -> str:
        """Tool description."""
        return "Extract text content from a PDF file (academic papers)"

    def __init__(self, working_dir: Optional[Path] = None):
        """Initialize PDF tool.

        Args:
            working_dir: Working directory for resolving relative paths
        """
        self.working_dir = working_dir or Path.cwd()

    def extract_text(self, file_path: str) -> dict[str, Any]:
        """Extract text content from a PDF file.

        Args:
            file_path: Path to the PDF file (absolute or relative to working_dir)

        Returns:
            Dict with:
                - success: bool
                - content: str (full extracted text)
                - sections: list[dict] (detected sections with titles and content)
                - metadata: dict (title, authors, etc. if detected)
                - error: str (if success is False)
        """
        try:
            # Resolve path
            path = Path(file_path)
            if not path.is_absolute():
                path = self.working_dir / path

            if not path.exists():
                return {
                    "success": False,
                    "error": f"PDF file not found: {path}",
                    "content": "",
                    "sections": [],
                    "metadata": {},
                }

            if not path.suffix.lower() == ".pdf":
                return {
                    "success": False,
                    "error": f"File is not a PDF: {path}",
                    "content": "",
                    "sections": [],
                    "metadata": {},
                }

            # Try pypdf first, fall back to pdfplumber
            try:
                return self._extract_with_pypdf(path)
            except ImportError:
                try:
                    return self._extract_with_pdfplumber(path)
                except ImportError:
                    return {
                        "success": False,
                        "error": "No PDF library available. Install pypdf or pdfplumber.",
                        "content": "",
                        "sections": [],
                        "metadata": {},
                    }

        except Exception as e:
            return {
                "success": False,
                "error": f"Failed to extract PDF: {str(e)}",
                "content": "",
                "sections": [],
                "metadata": {},
            }

    def _extract_with_pypdf(self, path: Path) -> dict[str, Any]:
        """Extract text using pypdf library."""
        from pypdf import PdfReader

        reader = PdfReader(str(path))

        # Extract metadata
        metadata = {}
        if reader.metadata:
            metadata = {
                "title": reader.metadata.get("/Title", ""),
                "author": reader.metadata.get("/Author", ""),
                "subject": reader.metadata.get("/Subject", ""),
                "creator": reader.metadata.get("/Creator", ""),
            }
            # Clean up empty values
            metadata = {k: v for k, v in metadata.items() if v}

        # Extract text from all pages
        full_text = []
        for page_num, page in enumerate(reader.pages):
            page_text = page.extract_text()
            if page_text:
                full_text.append(f"--- Page {page_num + 1} ---\n{page_text}")

        content = "\n\n".join(full_text)

        # Detect sections
        sections = self._detect_sections(content)

        return {
            "success": True,
            "content": content,
            "sections": sections,
            "metadata": metadata,
            "page_count": len(reader.pages),
        }

    def _extract_with_pdfplumber(self, path: Path) -> dict[str, Any]:
        """Extract text using pdfplumber library (better for complex layouts)."""
        import pdfplumber

        with pdfplumber.open(str(path)) as pdf:
            # Extract metadata
            metadata = {}
            if pdf.metadata:
                metadata = {
                    "title": pdf.metadata.get("Title", ""),
                    "author": pdf.metadata.get("Author", ""),
                    "subject": pdf.metadata.get("Subject", ""),
                    "creator": pdf.metadata.get("Creator", ""),
                }
                metadata = {k: v for k, v in metadata.items() if v}

            # Extract text from all pages
            full_text = []
            for page_num, page in enumerate(pdf.pages):
                page_text = page.extract_text()
                if page_text:
                    full_text.append(f"--- Page {page_num + 1} ---\n{page_text}")

            content = "\n\n".join(full_text)

            # Detect sections
            sections = self._detect_sections(content)

            return {
                "success": True,
                "content": content,
                "sections": sections,
                "metadata": metadata,
                "page_count": len(pdf.pages),
            }

    def _detect_sections(self, content: str) -> list[dict[str, Any]]:
        """Detect common academic paper sections.

        Args:
            content: Full text content

        Returns:
            List of detected sections with title and content
        """
        # Common section patterns in academic papers
        section_patterns = [
            r"^(?:(\d+\.?\s*)?)(Abstract)\s*$",
            r"^(?:(\d+\.?\s*)?)(Introduction)\s*$",
            r"^(?:(\d+\.?\s*)?)(Related\s+Work)\s*$",
            r"^(?:(\d+\.?\s*)?)(Background)\s*$",
            r"^(?:(\d+\.?\s*)?)(Methodology|Method|Methods)\s*$",
            r"^(?:(\d+\.?\s*)?)(Approach)\s*$",
            r"^(?:(\d+\.?\s*)?)(Model|Architecture)\s*$",
            r"^(?:(\d+\.?\s*)?)(Experiments?)\s*$",
            r"^(?:(\d+\.?\s*)?)(Results?)\s*$",
            r"^(?:(\d+\.?\s*)?)(Discussion)\s*$",
            r"^(?:(\d+\.?\s*)?)(Conclusion|Conclusions)\s*$",
            r"^(?:(\d+\.?\s*)?)(References|Bibliography)\s*$",
            r"^(?:(\d+\.?\s*)?)(Appendix|Appendices)\s*$",
        ]

        # Combined pattern
        combined_pattern = "|".join(f"({p})" for p in section_patterns)

        sections = []
        lines = content.split("\n")
        current_section = None
        current_content = []

        for line in lines:
            # Check if this line is a section header
            is_header = False
            for pattern in section_patterns:
                match = re.match(pattern, line.strip(), re.IGNORECASE)
                if match:
                    # Save previous section
                    if current_section:
                        sections.append({
                            "title": current_section,
                            "content": "\n".join(current_content).strip(),
                        })

                    # Start new section
                    current_section = line.strip()
                    current_content = []
                    is_header = True
                    break

            if not is_header and current_section:
                current_content.append(line)

        # Add last section
        if current_section:
            sections.append({
                "title": current_section,
                "content": "\n".join(current_content).strip(),
            })

        return sections

    def execute(self, **kwargs) -> dict[str, Any]:
        """Execute the tool (BaseTool interface)."""
        file_path = kwargs.get("file_path", "")
        return self.extract_text(file_path)
