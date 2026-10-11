#Requires -Version 5.1

[CmdletBinding()]
param(
    [string]$CredentialPath = (Join-Path $env:LOCALAPPDATA 'HFT-Solana\paper-provider.clixml')
)

if ($env:OS -ne 'Windows_NT') {
    throw 'This credential file uses Windows DPAPI. Run under the Windows user that saved it.'
}

# Keep decrypted values out of the caller's PowerShell variables and output.
& {
    param([string]$Path)

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw 'Provider credentials are missing. Follow docs/LOCAL_SETUP.md to configure access.'
    }

    $provider = Import-Clixml -LiteralPath $Path -ErrorAction Stop
    if ($provider.RpcUrl -isnot [Security.SecureString] -or
        $provider.XToken -isnot [Security.SecureString]) {
        throw 'Expected encrypted SecureString values for RpcUrl and XToken.'
    }

    try {
        foreach ($item in @(
            @{ Name = 'HFT_SOLANA_RPC_URL'; Value = $provider.RpcUrl },
            @{ Name = 'HFT_YELLOWSTONE_X_TOKEN'; Value = $provider.XToken }
        )) {
            $pointer = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($item.Value)
            try {
                [Environment]::SetEnvironmentVariable(
                    $item.Name,
                    [Runtime.InteropServices.Marshal]::PtrToStringBSTR($pointer),
                    [EnvironmentVariableTarget]::Process
                )
            } finally {
                [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($pointer)
            }
        }

        $env:HFT_YELLOWSTONE_ENDPOINT = [string]$provider.YellowstoneEndpoint
        $env:HFT_BOOTSTRAP_TIMEOUT_SECS = [string]$provider.BootstrapTimeoutSecs
        Write-Host 'Paper provider environment loaded (credentials hidden).'
        if (-not $provider.YellowstoneVerified) {
            Write-Host 'Yellowstone access and stream compatibility are not verified yet.'
        }
    } finally {
        $provider.RpcUrl.Dispose()
        $provider.XToken.Dispose()
    }
} $CredentialPath
