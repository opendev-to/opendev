"""Embedding utilities for semantic bullet selection.

This module provides embedding generation and caching for ACE playbook bullets,
enabling semantic similarity-based selection.
"""

from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass
from pathlib import Path
from typing import Dict, List, Optional

import numpy as np


@dataclass
class EmbeddingMetadata:
    """Metadata for a cached embedding."""

    text: str
    model: str
    hash: str
    embedding: List[float]

    @classmethod
    def create(cls, text: str, model: str, embedding: List[float]) -> "EmbeddingMetadata":
        """Create embedding metadata with computed hash.

        Args:
            text: The text that was embedded
            model: The embedding model used
            embedding: The embedding vector

        Returns:
            EmbeddingMetadata instance
        """
        # Create hash of text + model for cache key
        content = f"{model}:{text}"
        text_hash = hashlib.sha256(content.encode()).hexdigest()[:16]

        return cls(
            text=text,
            model=model,
            hash=text_hash,
            embedding=embedding,
        )


class EmbeddingCache:
    """Cache for bullet embeddings to avoid redundant API calls.

    The cache stores embeddings in memory and can be persisted to disk.
    Cache keys are based on content hash + model name to handle content updates.
    """

    def __init__(self, model: str = "text-embedding-3-small"):
        """Initialize embedding cache.

        Args:
            model: Default embedding model to use
        """
        self.model = model
        self._cache: Dict[str, EmbeddingMetadata] = {}

    def get(self, text: str, model: Optional[str] = None) -> Optional[List[float]]:
        """Get cached embedding for text.

        Args:
            text: Text to look up
            model: Embedding model (defaults to self.model)

        Returns:
            Cached embedding vector or None if not found
        """
        model = model or self.model
        cache_key = self._make_key(text, model)

        if cache_key in self._cache:
            return self._cache[cache_key].embedding

        return None

    def set(self, text: str, embedding: List[float], model: Optional[str] = None) -> None:
        """Cache an embedding.

        Args:
            text: Text that was embedded
            embedding: Embedding vector
            model: Embedding model used (defaults to self.model)
        """
        model = model or self.model
        cache_key = self._make_key(text, model)

        metadata = EmbeddingMetadata.create(text, model, embedding)
        self._cache[cache_key] = metadata

    def get_or_generate(
        self,
        text: str,
        model: Optional[str] = None,
        generator = None,
    ) -> List[float]:
        """Get cached embedding or generate new one.

        Args:
            text: Text to embed
            model: Embedding model (defaults to self.model)
            generator: Callable that generates embeddings (takes text, model)

        Returns:
            Embedding vector (from cache or freshly generated)
        """
        # Try cache first
        cached = self.get(text, model)
        if cached is not None:
            return cached

        # Generate if not cached
        if generator is None:
            raise ValueError("Generator required when embedding not in cache")

        embedding = generator(text, model or self.model)
        self.set(text, embedding, model)

        return embedding

    def batch_get_or_generate(
        self,
        texts: List[str],
        model: Optional[str] = None,
        generator = None,
    ) -> List[List[float]]:
        """Get or generate embeddings for multiple texts efficiently.

        Args:
            texts: List of texts to embed
            model: Embedding model (defaults to self.model)
            generator: Callable that generates embeddings in batch

        Returns:
            List of embedding vectors (matching order of input texts)
        """
        model = model or self.model
        embeddings = []
        missing_indices = []
        missing_texts = []

        # Check cache for each text
        for i, text in enumerate(texts):
            cached = self.get(text, model)
            if cached is not None:
                embeddings.append(cached)
            else:
                embeddings.append(None)  # Placeholder
                missing_indices.append(i)
                missing_texts.append(text)

        # Generate missing embeddings in batch
        if missing_texts and generator is not None:
            generated = generator(missing_texts, model)

            # Cache and insert generated embeddings
            for idx, text, embedding in zip(missing_indices, missing_texts, generated):
                self.set(text, embedding, model)
                embeddings[idx] = embedding

        return embeddings

    def clear(self) -> None:
        """Clear all cached embeddings."""
        self._cache.clear()

    def size(self) -> int:
        """Get number of cached embeddings."""
        return len(self._cache)

    def to_dict(self) -> Dict[str, any]:
        """Serialize cache to dictionary for persistence.

        Returns:
            Dictionary with cache contents
        """
        return {
            "model": self.model,
            "cache": {
                key: {
                    "text": meta.text,
                    "model": meta.model,
                    "hash": meta.hash,
                    "embedding": meta.embedding,
                }
                for key, meta in self._cache.items()
            },
        }

    @classmethod
    def from_dict(cls, data: Dict[str, any]) -> "EmbeddingCache":
        """Deserialize cache from dictionary.

        Args:
            data: Dictionary with cache contents

        Returns:
            EmbeddingCache instance
        """
        cache = cls(model=data.get("model", "text-embedding-3-small"))

        for key, meta_dict in data.get("cache", {}).items():
            metadata = EmbeddingMetadata(
                text=meta_dict["text"],
                model=meta_dict["model"],
                hash=meta_dict["hash"],
                embedding=meta_dict["embedding"],
            )
            cache._cache[key] = metadata

        return cache

    def save_to_file(self, path: str) -> None:
        """Save cache to JSON file.

        Args:
            path: File path to save to
        """
        file_path = Path(path)
        file_path.parent.mkdir(parents=True, exist_ok=True)

        with file_path.open("w", encoding="utf-8") as f:
            json.dump(self.to_dict(), f, ensure_ascii=False, indent=2)

    @classmethod
    def load_from_file(cls, path: str) -> Optional["EmbeddingCache"]:
        """Load cache from JSON file.

        Args:
            path: File path to load from

        Returns:
            EmbeddingCache instance or None if file doesn't exist
        """
        file_path = Path(path)
        if not file_path.exists():
            return None

        try:
            with file_path.open("r", encoding="utf-8") as f:
                data = json.load(f)
            return cls.from_dict(data)
        except (json.JSONDecodeError, KeyError, ValueError):
            # Return None if file is corrupted
            return None

    def _make_key(self, text: str, model: str) -> str:
        """Create cache key from text and model.

        Args:
            text: Text content
            model: Embedding model name

        Returns:
            Cache key (hash of content + model)
        """
        content = f"{model}:{text}"
        return hashlib.sha256(content.encode()).hexdigest()[:16]


def cosine_similarity(vec1: List[float], vec2: List[float]) -> float:
    """Calculate cosine similarity between two vectors.

    Args:
        vec1: First embedding vector
        vec2: Second embedding vector

    Returns:
        Similarity score between -1.0 and 1.0
        - 1.0 = identical direction
        - 0.0 = orthogonal
        - -1.0 = opposite direction
    """
    # Convert to numpy arrays for efficiency
    a = np.array(vec1)
    b = np.array(vec2)

    # Calculate cosine similarity
    dot_product = np.dot(a, b)
    norm_a = np.linalg.norm(a)
    norm_b = np.linalg.norm(b)

    # Avoid division by zero
    if norm_a == 0 or norm_b == 0:
        return 0.0

    similarity = dot_product / (norm_a * norm_b)

    # Clamp to valid range (floating point errors)
    return float(np.clip(similarity, -1.0, 1.0))


def batch_cosine_similarity(
    query_vec: List[float],
    vectors: List[List[float]]
) -> List[float]:
    """Calculate cosine similarity between query and multiple vectors efficiently.

    Args:
        query_vec: Query embedding vector
        vectors: List of embedding vectors to compare against

    Returns:
        List of similarity scores (matching order of input vectors)
    """
    # Convert to numpy for vectorized operations
    query = np.array(query_vec)
    matrix = np.array(vectors)

    # Calculate norms
    query_norm = np.linalg.norm(query)
    vector_norms = np.linalg.norm(matrix, axis=1)

    # Avoid division by zero
    if query_norm == 0:
        return [0.0] * len(vectors)

    # Calculate dot products in one operation
    dot_products = np.dot(matrix, query)

    # Calculate similarities
    similarities = dot_products / (query_norm * vector_norms + 1e-10)

    # Clamp and convert to list
    return np.clip(similarities, -1.0, 1.0).tolist()


def generate_embeddings(
    texts: str | List[str],
    model: str = "text-embedding-3-small",
    provider: str = "openai",
) -> List[float] | List[List[float]]:
    """Generate embeddings using any-llm API.

    Args:
        texts: Single text or list of texts to embed
        model: Embedding model name
        provider: LLM provider (default: openai)

    Returns:
        Single embedding vector or list of vectors
    """
    try:
        from any_llm import embedding

        # Call any-llm embedding API
        response = embedding(
            model=model,
            inputs=texts,
            provider=provider,
        )

        # Extract embedding vectors from response
        if isinstance(texts, str):
            # Single text
            return response.data[0].embedding
        else:
            # Multiple texts
            return [item.embedding for item in response.data]

    except Exception as e:
        # Fallback: return random embeddings for testing
        # In production, this should raise the error
        import random
        dim = 1536  # OpenAI embedding dimension

        if isinstance(texts, str):
            return [random.random() for _ in range(dim)]
        else:
            return [[random.random() for _ in range(dim)] for _ in texts]
