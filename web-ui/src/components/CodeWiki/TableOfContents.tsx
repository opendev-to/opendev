interface TOCItem {
  id: string;
  title: string;
  level: number;
}

interface TableOfContentsProps {
  items: TOCItem[];
  activeId?: string;
}

export function TableOfContents({ items, activeId }: TableOfContentsProps) {
  if (items.length === 0) {
    return null;
  }

  return (
    <aside className="w-56 bg-white border-l border-gray-200 overflow-y-auto sticky top-0 h-[calc(100vh-6.5rem)] hidden xl:block">
      <div className="p-4">
        <h3 className="text-xs font-semibold text-gray-500 uppercase tracking-wider mb-3">
          On This Page
        </h3>
        <nav>
          <ul className="space-y-2">
            {items.map((item) => (
              <li key={item.id} style={{ paddingLeft: `${(item.level - 2) * 12}px` }}>
                <a
                  href={`#${item.id}`}
                  className={`block text-sm transition-colors ${
                    activeId === item.id
                      ? 'text-purple-600 font-medium'
                      : 'text-gray-600 hover:text-gray-900'
                  }`}
                >
                  {item.title}
                </a>
              </li>
            ))}
          </ul>
        </nav>
      </div>
    </aside>
  );
}