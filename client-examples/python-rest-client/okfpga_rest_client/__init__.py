"""ok-FPGA-Server REST Client Package.

Provides an async REST client for the ok-FPGA-Server, mDNS discovery,
and MQTT status publishing. Built against the OpenAPI 3.0 spec at
/api/openapi.yaml.

Usage:
    from okfpga_rest_client.client import OkFpgaRestClient

    async with OkFpgaRestClient("http://localhost:8088") as client:
        status = await client.get_server_status()
        print(status)
"""

__version__ = "0.1.0"
