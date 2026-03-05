"""WhatsApp Business API adapter (skeleton).

This adapter integrates with WhatsApp Business API to receive and send messages.
Requires WhatsApp Business account and API credentials.

TODO: Implement when ready to deploy to WhatsApp
- Set up WhatsApp Business API account
- Configure webhook endpoint
- Implement message webhook handler
- Handle media messages
- Support templates for initial outreach
"""

from typing import Any, Optional

from swecli.core.channels.base import ChannelAdapter, InboundMessage, OutboundMessage


class WhatsAppAdapter(ChannelAdapter):
    """WhatsApp Business API adapter (not yet implemented).

    Example configuration:
        adapter = WhatsAppAdapter(
            phone_number_id=os.environ["WA_PHONE_NUMBER_ID"],
            access_token=os.environ["WA_ACCESS_TOKEN"],
            webhook_verify_token=os.environ["WA_WEBHOOK_TOKEN"],
        )
        await adapter.start()

    Features to implement:
    - Webhook endpoint for incoming messages
    - Convert WhatsApp webhook payload → InboundMessage
    - Convert OutboundMessage → WhatsApp Cloud API call
    - Handle phone number formatting
    - Support media messages (images, documents, audio)
    - Handle message templates (for first contact)
    - Support WhatsApp Business features (buttons, lists)
    """

    def __init__(
        self,
        phone_number_id: str,
        access_token: str,
        webhook_verify_token: str,
        api_version: str = "v18.0",
    ):
        """Initialize WhatsApp adapter.

        Args:
            phone_number_id: WhatsApp Business phone number ID
            access_token: WhatsApp Business API access token
            webhook_verify_token: Token for webhook verification
            api_version: WhatsApp Cloud API version
        """
        self.channel_name = "whatsapp"
        self.phone_number_id = phone_number_id
        self.access_token = access_token
        self.webhook_verify_token = webhook_verify_token
        self.api_version = api_version

    async def start(self) -> None:
        """Start WhatsApp webhook handler (not implemented)."""
        raise NotImplementedError(
            "WhatsAppAdapter not yet implemented. "
            "See swecli/core/channels/whatsapp.py for TODO items."
        )

    async def send(self, delivery_context: dict[str, Any], message: OutboundMessage) -> None:
        """Send message to WhatsApp user (not implemented)."""
        raise NotImplementedError(
            "WhatsAppAdapter not yet implemented. "
            "See swecli/core/channels/whatsapp.py for TODO items."
        )

    async def stop(self) -> None:
        """Stop WhatsApp webhook handler (not implemented)."""
        pass  # No-op for now
