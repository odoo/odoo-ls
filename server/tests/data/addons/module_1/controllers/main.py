# -*- coding: utf-8 -*-
from odoo import http
from odoo.http import request


class TestController(http.Controller):
    
    @http.route('/test/request', type='http', auth='public')
    def test_request_type(self):
        """Test that request has correct type."""
        # Hovering over 'request' should show Request class
        req = request
        
        # Hovering over 'request.env' should show Environment | None
        env = request.env
        
        # Accessing models via request.env
        partner = request.env['res.partner']
        partners = request.env['res.partner'].search([])
