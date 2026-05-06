# 1. Define the Provider (Google)
terraform {
  required_providers {
    google = {
      source  = "hashicorp/google"
      version = "~> 5.0"
    }
  }
}

provider "google" {
  project = "project-ec21338a-215e-427c-995"
  region  = "asia-south1"
}

# 2. Create a VPC Network (Best practice for security)
resource "google_compute_network" "vpc_network" {
  name                    = "my-terraform-network"
  auto_create_subnetworks = true
}

# 3. Create the GKE Autopilot Cluster
resource "google_container_cluster" "primary" {
  name     = "autopilot-cluster-tf"
  location = "asia-south1"

  # Enabling Autopilot mode
  enable_autopilot = true

  network    = google_compute_network.vpc_network.name

  networking_mode = "VPC_NATIVE"
   ip_allocation_policy {
     cluster_ipv4_cidr_block  = "/16"
     services_ipv4_cidr_block = "/22"
   }

   release_channel {
     channel = "REGULAR"
   }
  deletion_protection = false
}

resource "google_compute_instance" "k6_runner" {
  name    = "k6-runner-tf"
  machine_type = "n4-standard-4"
  zone = "asia-south1-a"

  boot_disk {
    initialize_params {
      image = "ubuntu-os-cloud/ubuntu-2204-lts"
      size  = 20
    }
  }

  network_interface {
    network =  google_compute_network.vpc_network.name
    access_config {

    }
  }

  service_account {
      scopes = ["cloud-platform"]
    }

    metadata_startup_script = <<-EOF
    #! /bin/bash
    sleep 10

    apt-get update && apt-get install -y wget tar

    wget https://github.com/grafana/k6/releases/download/v0.50.0/k6-v0.50.0-linux-amd64.tar.gz

    tar -xzf k6-v0.50.0-linux-amd64.tar.gz

    sudo mv k6-v0.50.0-linux-amd64/k6 /usr/local/bin/

    rm -rf k6-v0.50.0-linux-amd64*

     EOF


}

resource "google_compute_firewall" "allow_ssh" {
  name    = "allow-ssh-tf"
  network = google_compute_network.vpc_network.name

  allow {
    protocol = "tcp"
    ports    = ["22"]
  }

  source_ranges = ["1.2.3.4/32"] # WARNING:  Good for testing.
}

output "k6_runner_public_ip" {
  value = google_compute_instance.k6_runner.network_interface[0].access_config[0].nat_ip
}

output "kubernetes_cluster_name" {
  value = google_container_cluster.primary.name
}

output "cluster_endpoint" {
  value = google_container_cluster.primary.endpoint
}
