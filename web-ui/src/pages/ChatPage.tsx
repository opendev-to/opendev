import { TopBar } from '../components/Layout/TopBar';
import { SessionsSidebar } from '../components/Layout/SessionsSidebar';
import { ChatInterface } from '../components/Chat/ChatInterface';
import { ApprovalDialog } from '../components/ApprovalDialog';
import { AskUserDialog } from '../components/Chat/AskUserDialog';
import { PlanApprovalDialog } from '../components/Chat/PlanApprovalDialog';

export function ChatPage() {
  return (
    <div className="h-screen flex flex-col bg-bg-100">
      <TopBar />
      <div className="flex-1 flex overflow-hidden">
        <SessionsSidebar />
        <main className="flex-1 flex flex-col overflow-hidden bg-bg-000">
          <ChatInterface />
        </main>
      </div>

      {/* Modals */}
      <ApprovalDialog />
      <AskUserDialog />
      <PlanApprovalDialog />
    </div>
  );
}
